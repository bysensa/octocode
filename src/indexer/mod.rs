// Copyright 2025 Muvon Un Limited
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Indexer module for Octodev
// Handles code indexing, embedding, and search functionality

pub mod graph_optimization;
pub mod graphrag; // GraphRAG generation for code relationships (modular implementation)
pub mod languages; // Language-specific processors
pub mod search; // Search functionality // Task-focused graph extraction and optimization

pub mod render_utils;
pub use graph_optimization::*;
pub use graphrag::*;
pub use languages::*;
pub use search::*;

use crate::config::Config;
use crate::embedding::calculate_unique_content_hash;
use crate::mcp::logging::{
	log_file_processing_error, log_indexing_progress, log_performance_metrics,
};
use crate::state;
use crate::state::SharedState;
use crate::store::{CodeBlock, DocumentBlock, Store, TextBlock};
pub use render_utils::*;
use std::fs;
// We're using ignore::WalkBuilder instead of walkdir::WalkDir
use anyhow::Result;
use ignore;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

#[derive(Debug, Serialize, Clone)]
pub struct FileSignature {
	pub path: String,
	pub language: String,
	pub file_comment: Option<String>,
	pub signatures: Vec<SignatureItem>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SignatureItem {
	pub kind: String,                // e.g., "function", "struct", "class", etc.
	pub name: String,                // Name of the item
	pub signature: String,           // Full signature
	pub description: Option<String>, // Comment if available
	pub start_line: usize,           // Start line number
	pub end_line: usize,             // End line number
}

/// Utility to create an ignore Walker that respects both .gitignore and .noindex files
pub struct NoindexWalker;

impl NoindexWalker {
	/// Creates a WalkBuilder that respects .gitignore and .noindex files
	/// ENHANCED: Better error handling and debugging for .noindex files
	pub fn create_walker(current_dir: &Path) -> ignore::WalkBuilder {
		let mut builder = ignore::WalkBuilder::new(current_dir);

		// Standard git ignore settings
		builder
			.hidden(true) // Ignore hidden files (like .git/, .vscode/, etc.)
			.git_ignore(true) // Respect .gitignore files
			.git_global(true) // Respect global git ignore files
			.git_exclude(true); // Respect .git/info/exclude files

		// Add .noindex support by adding it as an additional ignore file
		let noindex_path = current_dir.join(".noindex");
		if noindex_path.exists() {
			match builder.add_ignore(&noindex_path) {
				Some(e) => {
					eprintln!("Warning: Failed to load .noindex file: {}", e);
				}
				None => {
					// Successfully loaded - no need to be verbose
				}
			}
		}

		builder
	}

	/// Creates a GitignoreBuilder for checking individual files against both .gitignore and .noindex
	/// ENHANCED: Better error handling and debugging
	pub fn create_matcher(current_dir: &Path) -> Result<ignore::gitignore::Gitignore> {
		let mut builder = ignore::gitignore::GitignoreBuilder::new(current_dir);

		// Add .gitignore files
		let gitignore_path = current_dir.join(".gitignore");
		if gitignore_path.exists() {
			if let Some(e) = builder.add(&gitignore_path) {
				eprintln!("Warning: Failed to load .gitignore file: {}", e);
			} // Successfully loaded
		}

		// Add .noindex file if it exists
		let noindex_path = current_dir.join(".noindex");
		if noindex_path.exists() {
			if let Some(e) = builder.add(&noindex_path) {
				eprintln!("Warning: Failed to load .noindex file for matcher: {}", e);
			} // Successfully loaded
		}

		Ok(builder.build()?)
	}
}

/// Utility for consistent path handling - always returns relative paths
pub struct PathUtils;

impl PathUtils {
	/// Converts an absolute path to a relative path from the current directory
	/// Returns the relative path as a String, suitable for storage and display
	pub fn to_relative_string(path: &Path, current_dir: &Path) -> String {
		path.strip_prefix(current_dir)
			.unwrap_or(path)
			.to_string_lossy()
			.to_string()
	}

	/// Converts an absolute PathBuf to a relative path string from the current directory
	pub fn pathbuf_to_relative_string(path: &Path, current_dir: &Path) -> String {
		Self::to_relative_string(path, current_dir)
	}

	/// Creates a relative path for display purposes, ensuring it never shows absolute paths
	pub fn for_display(path: &Path, current_dir: &Path) -> String {
		let relative = Self::to_relative_string(path, current_dir);

		// If the path starts with '/', it means strip_prefix failed and we got an absolute path
		// In this case, just show the filename or a sanitized version
		if relative.starts_with('/') {
			if let Some(filename) = path.file_name() {
				filename.to_string_lossy().to_string()
			} else {
				"<unknown>".to_string()
			}
		} else {
			relative
		}
	}
}

/// Git utilities for repository management
pub mod git {
	use anyhow::Result;
	use std::path::Path;
	use std::process::Command;

	/// Check if current directory is a git repository root
	pub fn is_git_repo_root(path: &Path) -> bool {
		path.join(".git").exists()
	}

	/// Find git repository root from current path
	pub fn find_git_root(start_path: &Path) -> Option<std::path::PathBuf> {
		let mut current = start_path;
		loop {
			if is_git_repo_root(current) {
				return Some(current.to_path_buf());
			}

			if let Some(parent) = current.parent() {
				current = parent;
			} else {
				break;
			}
		}
		None
	}

	/// Get current git commit hash
	pub fn get_current_commit_hash(repo_path: &Path) -> Result<String> {
		let output = Command::new("git")
			.args(["rev-parse", "HEAD"])
			.current_dir(repo_path)
			.output()?;

		if !output.status.success() {
			return Err(anyhow::anyhow!(
				"Failed to get git commit hash: {}",
				String::from_utf8_lossy(&output.stderr)
			));
		}

		Ok(String::from_utf8(output.stdout)?.trim().to_string())
	}

	/// Get files changed between two commits (including unstaged changes)
	pub fn get_changed_files_since_commit(
		repo_path: &Path,
		since_commit: &str,
	) -> Result<Vec<String>> {
		let mut changed_files = std::collections::HashSet::new();

		// Get committed changes since the specified commit
		let output = Command::new("git")
			.args(["diff", "--name-only", &format!("{}..HEAD", since_commit)])
			.current_dir(repo_path)
			.output()?;

		if output.status.success() {
			let committed_files = String::from_utf8(output.stdout)?;
			for file in committed_files.lines() {
				if !file.trim().is_empty() {
					changed_files.insert(file.trim().to_string());
				}
			}
		}

		// Get unstaged changes
		let output = Command::new("git")
			.args(["diff", "--name-only"])
			.current_dir(repo_path)
			.output()?;

		if output.status.success() {
			let unstaged_files = String::from_utf8(output.stdout)?;
			for file in unstaged_files.lines() {
				if !file.trim().is_empty() {
					changed_files.insert(file.trim().to_string());
				}
			}
		}

		// Get staged changes
		let output = Command::new("git")
			.args(["diff", "--cached", "--name-only"])
			.current_dir(repo_path)
			.output()?;

		if output.status.success() {
			let staged_files = String::from_utf8(output.stdout)?;
			for file in staged_files.lines() {
				if !file.trim().is_empty() {
					changed_files.insert(file.trim().to_string());
				}
			}
		}

		// Get untracked files
		let output = Command::new("git")
			.args(["ls-files", "--others", "--exclude-standard"])
			.current_dir(repo_path)
			.output()?;

		if output.status.success() {
			let untracked_files = String::from_utf8(output.stdout)?;
			for file in untracked_files.lines() {
				if !file.trim().is_empty() {
					changed_files.insert(file.trim().to_string());
				}
			}
		}

		Ok(changed_files.into_iter().collect())
	}

	/// Get all changed files (committed + staged + unstaged + untracked)
	pub fn get_all_changed_files(repo_path: &Path) -> Result<Vec<String>> {
		let mut changed_files = std::collections::HashSet::new();

		// Get unstaged changes
		let output = Command::new("git")
			.args(["diff", "--name-only"])
			.current_dir(repo_path)
			.output()?;

		if output.status.success() {
			let unstaged_files = String::from_utf8(output.stdout)?;
			for file in unstaged_files.lines() {
				if !file.trim().is_empty() {
					changed_files.insert(file.trim().to_string());
				}
			}
		}

		// Get staged changes
		let output = Command::new("git")
			.args(["diff", "--cached", "--name-only"])
			.current_dir(repo_path)
			.output()?;

		if output.status.success() {
			let staged_files = String::from_utf8(output.stdout)?;
			for file in staged_files.lines() {
				if !file.trim().is_empty() {
					changed_files.insert(file.trim().to_string());
				}
			}
		}

		// Get untracked files
		let output = Command::new("git")
			.args(["ls-files", "--others", "--exclude-standard"])
			.current_dir(repo_path)
			.output()?;

		if output.status.success() {
			let untracked_files = String::from_utf8(output.stdout)?;
			for file in untracked_files.lines() {
				if !file.trim().is_empty() {
					changed_files.insert(file.trim().to_string());
				}
			}
		}

		Ok(changed_files.into_iter().collect())
	}
}

/// Get file modification time as seconds since Unix epoch
pub fn get_file_mtime(file_path: &std::path::Path) -> Result<u64> {
	let metadata = std::fs::metadata(file_path)?;
	let mtime = metadata
		.modified()?
		.duration_since(std::time::UNIX_EPOCH)?
		.as_secs();
	Ok(mtime)
}

/// Check if file should be skipped based on modification time
async fn should_skip_file(
	store: &Store,
	file_path: &str,
	actual_mtime: u64,
	force_reindex: bool,
) -> Result<bool> {
	// Always process if force reindex
	if force_reindex {
		return Ok(false);
	}

	// Check if we have stored metadata for this file
	if let Ok(Some(stored_mtime)) = store.get_file_mtime(file_path).await {
		// Skip if file hasn't been modified
		if actual_mtime <= stored_mtime {
			return Ok(true);
		}
	}

	// Process the file (either new or modified)
	Ok(false)
}

// Detect language based on file extension
pub fn detect_language(path: &std::path::Path) -> Option<&str> {
	match path.extension()?.to_str()? {
		"rs" => Some("rust"),
		"php" => Some("php"),
		"py" => Some("python"),
		"js" => Some("javascript"),
		"ts" => Some("typescript"),
		"jsx" | "tsx" => Some("typescript"),
		"json" => Some("json"),
		"go" => Some("go"),
		"cpp" | "cc" | "cxx" | "c++" | "hpp" | "h" => Some("cpp"),
		"sh" | "bash" => Some("bash"),
		"rb" => Some("ruby"),
		"md" => Some("markdown"),
		_ => None,
	}
}

/// Function to extract file signatures
pub fn extract_file_signatures(files: &[PathBuf]) -> Result<Vec<FileSignature>> {
	let mut all_signatures = Vec::new();
	let mut parser = Parser::new();
	let current_dir = std::env::current_dir()?;

	for file_path in files {
		if let Some(language) = detect_language(file_path) {
			// Get the language implementation
			let lang_impl = match languages::get_language(language) {
				Some(impl_) => impl_,
				None => continue, // Skip unsupported languages
			};

			// Set the parser language
			parser.set_language(&lang_impl.get_ts_language())?;

			// Read file contents
			if let Ok(contents) = fs::read_to_string(file_path) {
				// Create a relative path for display using our utility
				let display_path = PathUtils::for_display(file_path, &current_dir);

				// Parse the file
				let tree = parser
					.parse(&contents, None)
					.unwrap_or_else(|| parser.parse("", None).unwrap());

				// Extract signatures from the file
				let signatures =
					extract_signatures(tree.root_node(), &contents, lang_impl.as_ref());

				// Extract file-level comment if present
				let file_comment = extract_file_comment(tree.root_node(), &contents);

				// Add to our results
				all_signatures.push(FileSignature {
					path: display_path,
					language: lang_impl.name().to_string(),
					file_comment,
					signatures,
				});
			}
		}
	}

	Ok(all_signatures)
}

/// Extract signatures from a parsed file
fn extract_signatures(
	node: Node,
	contents: &str,
	lang_impl: &dyn languages::Language,
) -> Vec<SignatureItem> {
	let mut signatures = Vec::new();
	let meaningful_kinds = lang_impl.get_meaningful_kinds();

	// Create a visitor function to traverse the tree
	fn visit_node(
		node: Node,
		contents: &str,
		lang_impl: &dyn languages::Language,
		meaningful_kinds: &[&str],
		signatures: &mut Vec<SignatureItem>,
	) {
		let node_kind = node.kind();

		// Check if this node is a meaningful declaration
		if meaningful_kinds.contains(&node_kind) {
			// Get the line numbers
			let start_line = node.start_position().row;
			let end_line = node.end_position().row;

			// Extract the name of the item (function name, struct name, etc.)
			let name = extract_name(node, contents, lang_impl);

			// Extract the preceding comment if available
			let description = extract_preceding_comment(node, contents);

			if let Some(name) = name {
				// Get the full signature text
				let sig_text = node_text(node, contents);

				// Map tree-sitter node kinds to our simplified kinds
				let kind = map_node_kind_to_simple(node_kind);

				signatures.push(SignatureItem {
					kind,
					name,
					signature: sig_text,
					description,
					start_line,
					end_line,
				});
			}
		}

		// Recursively process children
		let mut cursor = node.walk();
		if cursor.goto_first_child() {
			loop {
				visit_node(
					cursor.node(),
					contents,
					lang_impl,
					meaningful_kinds,
					signatures,
				);
				if !cursor.goto_next_sibling() {
					break;
				}
			}
		}
	}

	// Start traversal from the root
	visit_node(
		node,
		contents,
		lang_impl,
		&meaningful_kinds,
		&mut signatures,
	);

	// Sort by line number for a consistent order
	signatures.sort_by_key(|sig| sig.start_line);

	signatures
}

/// Extract the name of a declaration node (function, class, etc.)
fn extract_name(node: Node, contents: &str, lang_impl: &dyn languages::Language) -> Option<String> {
	// Look for identifier nodes
	for child in node.children(&mut node.walk()) {
		if child.kind() == "identifier"
			|| child.kind().contains("name")
			|| child.kind().contains("function_name")
		{
			if let Ok(name) = child.utf8_text(contents.as_bytes()) {
				if !name.is_empty() {
					return Some(name.to_string());
				}
			}
		}
	}

	// Fall back to using language-specific symbol extraction
	let symbols = lang_impl.extract_symbols(node, contents);
	symbols.into_iter().next()
}

/// Extract a preceding comment if available
fn extract_preceding_comment(node: Node, contents: &str) -> Option<String> {
	if let Some(parent) = node.parent() {
		let mut siblings = Vec::new();
		let mut cursor = parent.walk();

		if cursor.goto_first_child() {
			loop {
				let current = cursor.node();
				if current.id() == node.id() {
					break;
				}
				siblings.push(current);
				if !cursor.goto_next_sibling() {
					break;
				}
			}
		}

		// Check the last sibling before our node
		if let Some(last) = siblings.last() {
			if last.kind().contains("comment") {
				if let Ok(comment) = last.utf8_text(contents.as_bytes()) {
					// Clean up comment markers
					let comment = comment
						.trim()
						.trim_start_matches("/")
						.trim_start_matches("*")
						.trim_start_matches("/")
						.trim_end_matches("*/")
						.trim();
					return Some(comment.to_string());
				}
			}
		}
	}
	None
}

/// Extract a file-level comment (usually at the top of the file)
fn extract_file_comment(root: Node, contents: &str) -> Option<String> {
	let mut cursor = root.walk();
	if cursor.goto_first_child() {
		// Check if the first node is a comment
		let first = cursor.node();
		if first.kind().contains("comment") {
			if let Ok(comment) = first.utf8_text(contents.as_bytes()) {
				// Clean up comment markers
				let comment = comment
					.trim()
					.trim_start_matches("/")
					.trim_start_matches("*")
					.trim_start_matches("/")
					.trim_end_matches("*/")
					.trim();
				return Some(comment.to_string());
			}
		}
	}
	None
}

/// Get the full text of a node
fn node_text(node: Node, contents: &str) -> String {
	if let Ok(text) = node.utf8_text(contents.as_bytes()) {
		text.to_string()
	} else {
		// Fall back to byte range if UTF-8 conversion fails
		let start_byte = node.start_byte();
		let end_byte = node.end_byte();
		let content_bytes = contents.as_bytes();

		if start_byte < end_byte && end_byte <= content_bytes.len() {
			String::from_utf8_lossy(&content_bytes[start_byte..end_byte]).to_string()
		} else {
			String::new()
		}
	}
}

/// Parse markdown content and split it into meaningful chunks by headers
pub fn parse_markdown_content(contents: &str, file_path: &str) -> Vec<DocumentBlock> {
	let mut document_blocks = Vec::new();
	let lines: Vec<&str> = contents.lines().collect();

	let mut current_title = "Introduction".to_string();
	let mut current_level = 1;
	let mut current_content = String::new();
	let mut start_line = 0;

	for (line_num, line) in lines.iter().enumerate() {
		let trimmed = line.trim_start();

		// Check if this line is a header
		if trimmed.starts_with('#') {
			// Save the previous section if it has content
			if !current_content.trim().is_empty() {
				let content_hash = calculate_unique_content_hash(&current_content, file_path);
				document_blocks.push(DocumentBlock {
					path: file_path.to_string(),
					title: current_title.clone(),
					content: current_content.trim().to_string(),
					level: current_level,
					start_line,
					end_line: line_num.saturating_sub(1),
					hash: content_hash,
					distance: None,
				});
			}

			// Start a new section
			let header_level = trimmed.chars().take_while(|&c| c == '#').count();
			current_title = trimmed.trim_start_matches('#').trim().to_string();
			if current_title.is_empty() {
				current_title = format!("Section at line {}", line_num + 1);
			}
			current_level = header_level;
			current_content.clear();
			start_line = line_num;
		} else {
			// Add non-header line to current content
			current_content.push_str(line);
			current_content.push('\n');
		}
	}

	// Don't forget the last section
	if !current_content.trim().is_empty() {
		let content_hash = calculate_unique_content_hash(&current_content, file_path);
		document_blocks.push(DocumentBlock {
			path: file_path.to_string(),
			title: current_title,
			content: current_content.trim().to_string(),
			level: current_level,
			start_line,
			end_line: lines.len().saturating_sub(1),
			hash: content_hash,
			distance: None,
		});
	}

	// If no headers were found, treat the entire content as one document
	if document_blocks.is_empty() && !contents.trim().is_empty() {
		let content_hash = calculate_unique_content_hash(contents, file_path);
		document_blocks.push(DocumentBlock {
			path: file_path.to_string(),
			title: std::path::Path::new(file_path)
				.file_stem()
				.and_then(|s| s.to_str())
				.unwrap_or("Document")
				.to_string(),
			content: contents.trim().to_string(),
			level: 1,
			start_line: 0,
			end_line: lines.len().saturating_sub(1),
			hash: content_hash,
			distance: None,
		});
	}

	document_blocks
}

/// Map tree-sitter node kinds to simpler, unified kinds for display
fn map_node_kind_to_simple(kind: &str) -> String {
	match kind {
		k if k.contains("function") => "function".to_string(),
		k if k.contains("method") => "method".to_string(),
		k if k.contains("class") => "class".to_string(),
		k if k.contains("struct") => "struct".to_string(),
		k if k.contains("enum") => "enum".to_string(),
		k if k.contains("interface") => "interface".to_string(),
		k if k.contains("trait") => "trait".to_string(),
		k if k.contains("mod") || k.contains("module") => "module".to_string(),
		k if k.contains("const") => "constant".to_string(),
		k if k.contains("macro") => "macro".to_string(),
		k if k.contains("type") => "type".to_string(),
		_ => kind.to_string(), // Fall back to the original kind
	}
}

/// Render signatures and search results as markdown output (more efficient for AI tools)
// Rendering functions have been moved to src/indexer/render_utils.rs
// Main function to index files with optional git optimization
pub async fn index_files(
	store: &Store,
	state: SharedState,
	config: &Config,
	git_repo_root: Option<&Path>,
) -> Result<()> {
	let current_dir = state.read().current_directory.clone();
	let mut code_blocks_batch = Vec::new();
	let mut text_blocks_batch = Vec::new();
	let mut document_blocks_batch = Vec::new();
	let mut all_code_blocks = Vec::new(); // Store all code blocks for GraphRAG

	let mut embedding_calls = 0;

	// Log indexing start
	log_indexing_progress(
		"indexing_start",
		0,
		0,
		Some(&current_dir.display().to_string()),
		0,
	);

	// Initialize GraphRAG state if enabled
	{
		let mut state_guard = state.write();
		state_guard.graphrag_enabled = config.graphrag.enabled;
		state_guard.graphrag_blocks = 0;
		state_guard.counting_files = true;
		state_guard.status_message = "Counting files...".to_string();
	}

	// Get force_reindex flag from state
	let force_reindex = state.read().force_reindex;

	// Git-based optimization: Get changed files if we have a git repository
	let git_changed_files = if let Some(git_root) = git_repo_root {
		if !force_reindex {
			// Try to get the last indexed commit
			if let Ok(Some(last_commit)) = store.get_last_commit_hash().await {
				// Get current commit
				if let Ok(current_commit) = git::get_current_commit_hash(git_root) {
					if last_commit != current_commit {
						// Get files changed since last indexed commit
						match git::get_changed_files_since_commit(git_root, &last_commit) {
							Ok(changed_files) => {
								println!(
									"🚀 Git optimization: Found {} changed files since last commit",
									changed_files.len()
								);
								Some(
									changed_files
										.into_iter()
										.collect::<std::collections::HashSet<_>>(),
								)
							}
							Err(e) => {
								eprintln!(
									"Warning: Could not get git changes, indexing all files: {}",
									e
								);
								None
							}
						}
					} else {
						// Same commit, only check for uncommitted changes
						match git::get_all_changed_files(git_root) {
							Ok(changed_files) => {
								if changed_files.is_empty() {
									println!("✅ No changes since last index, skipping");
									{
										let mut state_guard = state.write();
										state_guard.indexing_complete = true;
									}
									return Ok(());
								} else {
									println!(
										"🚀 Git optimization: Found {} uncommitted changes",
										changed_files.len()
									);
									Some(
										changed_files
											.into_iter()
											.collect::<std::collections::HashSet<_>>(),
									)
								}
							}
							Err(e) => {
								eprintln!(
									"Warning: Could not get git status, indexing all files: {}",
									e
								);
								None
							}
						}
					}
				} else {
					None
				}
			} else {
				// No previous commit stored, need to index all files for baseline
				println!("📋 First-time git indexing: indexing all files");
				None
			}
		} else {
			// Force reindex, ignore git optimization
			None
		}
	} else {
		// No git repository, use file-based optimization
		None
	};

	// First, check for deleted files and files now ignored by .noindex and remove their blocks from the database
	// Note: Skip cleanup for force reindex or if this appears to be a fresh database
	let should_cleanup_deleted_files = {
		let force_reindex = state.read().force_reindex;
		if force_reindex {
			false // Skip cleanup during force reindex
		} else {
			// Check if this looks like a fresh database by seeing if we have any indexed files
			let indexed_files = store.get_all_indexed_file_paths().await.unwrap_or_default();
			!indexed_files.is_empty() // Only cleanup if we have existing indexed files
		}
	};

	if should_cleanup_deleted_files {
		{
			let mut state_guard = state.write();
			state_guard.status_message = "Checking for deleted and ignored files...".to_string();
		}

		// Log cleanup phase start
		log_indexing_progress("cleanup", 0, 0, None, 0);

		// Get all indexed file paths from the database
		let indexed_files = store.get_all_indexed_file_paths().await?;

		// Create ignore matcher to check against .noindex and .gitignore patterns
		let ignore_matcher = NoindexWalker::create_matcher(&current_dir)?;

		// Check which indexed files no longer exist on disk OR are now ignored
		let mut files_to_remove = Vec::new();
		for indexed_file in &indexed_files {
			// Always treat indexed paths as relative to current directory
			let absolute_path = current_dir.join(indexed_file);

			// Check if file was deleted
			if !absolute_path.exists() {
				files_to_remove.push((indexed_file.clone(), "deleted".to_string()));
			} else {
				// Check if file is now ignored by .noindex or .gitignore patterns
				let is_ignored = ignore_matcher
					.matched(&absolute_path, absolute_path.is_dir())
					.is_ignore();
				if is_ignored {
					files_to_remove.push((indexed_file.clone(), "ignored".to_string()));
				}
			}
		}

		// Remove blocks for files that were deleted or are now ignored
		if !files_to_remove.is_empty() {
			for (file_to_remove, _reason) in &files_to_remove {
				// Use the comprehensive removal function which handles all tables
				match store.remove_blocks_by_path(file_to_remove).await {
					Ok(_) => {} // Success - no need to be verbose
					Err(e) => {
						eprintln!(
							"ERROR: Failed to remove blocks for {}: {}",
							file_to_remove, e
						);
						// Continue with other files even if one fails
					}
				}
			}

			// Force flush to ensure deletions are persisted
			store.flush().await?;
		}

		{
			let mut state_guard = state.write();
			state_guard.status_message = "".to_string();
		}
	} // End of deleted files cleanup

	// First pass: count all files to be processed
	{
		let mut state_guard = state.write();
		state_guard.status_message = "Counting files...".to_string();
	}

	// Count files based on whether git optimization is active
	let total_files = if let Some(ref changed_files) = git_changed_files {
		// Git optimization active: count only changed files that are indexable
		let mut count = 0;
		for changed_file in changed_files {
			let absolute_path = current_dir.join(changed_file);
			if absolute_path.exists()
				&& absolute_path.is_file()
				&& (detect_language(&absolute_path).is_some()
					|| is_allowed_text_extension(&absolute_path))
			{
				count += 1;
			}
		}
		count
	} else {
		// No git optimization: count all indexable files
		count_indexable_files(&current_dir)?
	};

	{
		let mut state_guard = state.write();
		state_guard.total_files = total_files;
		state_guard.counting_files = false;
		state_guard.status_message = "".to_string(); // Clear the status message
	}

	// Second pass: actually process the files
	// Use NoindexWalker to respect both .gitignore and .noindex files
	let walker = NoindexWalker::create_walker(&current_dir).build();

	// Log file processing phase start
	log_indexing_progress("file_processing", 0, total_files, None, 0);

	for result in walker {
		let entry = match result {
			Ok(entry) => entry,
			Err(_) => continue,
		};

		// Skip directories, only process files
		if !entry.file_type().is_some_and(|ft| ft.is_file()) {
			continue;
		}

		// Create relative path from the current directory using our utility
		let file_path = PathUtils::to_relative_string(entry.path(), &current_dir);

		// GIT OPTIMIZATION: Skip files not in the changed set (if git optimization is active)
		if let Some(ref changed_files) = git_changed_files {
			if !changed_files.contains(&file_path) {
				// File not in git changes, skip processing entirely
				continue;
			}
		}

		// OPTIMIZATION: Check file modification time to skip unchanged files
		let force_reindex = state.read().force_reindex;
		if let Ok(actual_mtime) = get_file_mtime(entry.path()) {
			if let Ok(should_skip) =
				should_skip_file(store, &file_path, actual_mtime, force_reindex).await
			{
				if should_skip {
					// File hasn't changed, skip processing entirely
					continue;
				}
			}
		}

		if let Some(language) = detect_language(entry.path()) {
			match fs::read_to_string(entry.path()) {
				Ok(contents) => {
					// Store the file modification time after successful processing
					let file_processed;

					if language == "markdown" {
						// Handle markdown files specially - index as document blocks
						process_markdown_file_differential(
							store,
							&contents,
							&file_path,
							&mut document_blocks_batch,
							config,
							state.clone(),
						)
						.await?;
						file_processed = true;
					} else {
						// Handle code files - index as semantic code blocks only
						let ctx = ProcessFileContext {
							store,
							config,
							state: state.clone(),
						};
						process_file_differential(
							&ctx,
							&contents,
							&file_path,
							language,
							&mut code_blocks_batch,
							&mut text_blocks_batch, // Will remain empty for code files
							&mut all_code_blocks,
						)
						.await?;
						file_processed = true;
					}

					// Store file modification time after successful processing
					if file_processed {
						if let Ok(actual_mtime) = get_file_mtime(entry.path()) {
							let _ = store.store_file_metadata(&file_path, actual_mtime).await;
						}
					}

					state.write().indexed_files += 1;

					// Log progress periodically for code files
					let current_files = state.read().indexed_files;
					if current_files % 50 == 0 || current_files == total_files {
						log_indexing_progress(
							"file_processing",
							current_files,
							total_files,
							Some(&file_path),
							embedding_calls,
						);
					}

					// Process batches when they reach the batch size
					if code_blocks_batch.len() >= config.index.embeddings_batch_size {
						embedding_calls += code_blocks_batch.len();
						process_code_blocks_batch(store, &code_blocks_batch, config).await?;
						code_blocks_batch.clear();
					}
					// Only process text_blocks_batch if we have any (from unsupported files)
					if text_blocks_batch.len() >= config.index.embeddings_batch_size {
						embedding_calls += text_blocks_batch.len();
						process_text_blocks_batch(store, &text_blocks_batch, config).await?;
						text_blocks_batch.clear();
					}
					if document_blocks_batch.len() >= config.index.embeddings_batch_size {
						embedding_calls += document_blocks_batch.len();
						process_document_blocks_batch(store, &document_blocks_batch, config)
							.await?;
						document_blocks_batch.clear();
					}
				}
				Err(e) => {
					// Log file reading error
					log_file_processing_error(&file_path, "read_file", &e);
				}
			}
		} else {
			// Handle unsupported file types as chunked text
			// First check if the file extension is in our whitelist
			if is_allowed_text_extension(entry.path()) {
				if let Ok(contents) = fs::read_to_string(entry.path()) {
					// Only process files that are likely to contain readable text
					if is_text_file(&contents) {
						process_text_file_differential(
							store,
							&contents,
							&file_path,
							&mut text_blocks_batch,
							config,
							state.clone(),
						)
						.await?;

						// Store file modification time after successful processing
						if let Ok(actual_mtime) = get_file_mtime(entry.path()) {
							let _ = store.store_file_metadata(&file_path, actual_mtime).await;
						}

						state.write().indexed_files += 1;

						// Log progress periodically for text files
						let current_files = state.read().indexed_files;
						if current_files % 50 == 0 || current_files == total_files {
							log_indexing_progress(
								"file_processing",
								current_files,
								total_files,
								Some(&file_path),
								embedding_calls,
							);
						}

						// Process batch when it reaches the batch size
						if text_blocks_batch.len() >= config.index.embeddings_batch_size {
							embedding_calls += text_blocks_batch.len();
							process_text_blocks_batch(store, &text_blocks_batch, config).await?;
							text_blocks_batch.clear();
						}
					}
				}
			}
		}
	}

	// Process remaining batches
	if !code_blocks_batch.is_empty() {
		process_code_blocks_batch(store, &code_blocks_batch, config).await?;
		embedding_calls += code_blocks_batch.len();
	}
	// Only process text_blocks_batch if we have any (from unsupported files)
	if !text_blocks_batch.is_empty() {
		process_text_blocks_batch(store, &text_blocks_batch, config).await?;
		embedding_calls += text_blocks_batch.len();
	}
	if !document_blocks_batch.is_empty() {
		process_document_blocks_batch(store, &document_blocks_batch, config).await?;
		embedding_calls += document_blocks_batch.len();
	}

	// Build GraphRAG from all collected code blocks if enabled and if we found any blocks
	if config.graphrag.enabled && !all_code_blocks.is_empty() {
		{
			let mut state_guard = state.write();
			state_guard.status_message = "Building GraphRAG knowledge graph...".to_string();
		}

		// Log GraphRAG phase start
		log_indexing_progress(
			"graphrag_build",
			state.read().indexed_files,
			total_files,
			None,
			embedding_calls,
		);

		// Initialize GraphBuilder
		let graph_builder = graphrag::GraphBuilder::new(config.clone()).await?;

		// Process code blocks to build the graph
		graph_builder
			.process_code_blocks(&all_code_blocks, Some(state.clone()))
			.await?;

		// Update final state
		{
			let mut state_guard = state.write();
			state_guard.status_message = "".to_string();
		}
	}

	{
		let mut state_guard = state.write();
		state_guard.indexing_complete = true;
		state_guard.embedding_calls = embedding_calls;
	}

	// Log indexing completion
	let final_files = state.read().indexed_files;
	log_indexing_progress(
		"indexing_complete",
		final_files,
		total_files,
		None,
		embedding_calls,
	);

	// Store current git commit hash for future optimization
	if let Some(git_root) = git_repo_root {
		if let Ok(current_commit) = git::get_current_commit_hash(git_root) {
			if let Err(e) = store.store_git_metadata(&current_commit).await {
				eprintln!("Warning: Could not store git metadata: {}", e);
			}
		}
	}

	// Flush the store to ensure all data is persisted
	store.flush().await?;

	Ok(())
}

// Helper function to count indexable files
fn count_indexable_files(current_dir: &std::path::Path) -> Result<usize> {
	let mut count = 0;

	// Use NoindexWalker to respect both .gitignore and .noindex files
	let walker = NoindexWalker::create_walker(current_dir).build();

	for result in walker {
		let entry = match result {
			Ok(entry) => entry,
			Err(_) => continue,
		};

		// Skip directories, only count files
		if !entry.file_type().is_some_and(|ft| ft.is_file()) {
			continue;
		}

		// Check if this file would be indexed
		if detect_language(entry.path()).is_some() || is_allowed_text_extension(entry.path()) {
			count += 1;
		}
	}

	Ok(count)
}

// Function to handle file changes (for watch mode)
pub async fn handle_file_change(store: &Store, file_path: &str, config: &Config) -> Result<()> {
	// Create a state for tracking changes
	let state = state::create_shared_state();
	{
		let mut state_guard = state.write();
		state_guard.graphrag_enabled = config.graphrag.enabled;
		state_guard.graphrag_blocks = 0;
	}

	// First, let's remove any existing code blocks for this file path
	store.remove_blocks_by_path(file_path).await?;

	// Now, if the file still exists, check if it should be indexed based on ignore rules
	let path = std::path::Path::new(file_path);
	if path.exists() {
		// Get the current directory for proper relative path handling
		let current_dir = std::env::current_dir()?;

		// Convert relative path to absolute for ignore checking
		let absolute_path = if path.is_absolute() {
			path.to_path_buf()
		} else {
			current_dir.join(path)
		};

		// Create a matcher that respects both .gitignore and .noindex rules
		if let Ok(matcher) = NoindexWalker::create_matcher(&current_dir) {
			// Check if the file should be ignored
			if matcher
				.matched(&absolute_path, absolute_path.is_dir())
				.is_ignore()
			{
				// File is in ignore patterns, so don't index it
				return Ok(());
			}
		}

		// File is not ignored, so proceed with indexing
		if let Some(language) = detect_language(&absolute_path) {
			if let Ok(contents) = fs::read_to_string(&absolute_path) {
				// Ensure we use relative path for storage
				let relative_file_path =
					PathUtils::to_relative_string(&absolute_path, &current_dir);

				if language == "markdown" {
					// Handle markdown files specially
					let mut document_blocks_batch = Vec::new();
					process_markdown_file(
						store,
						&contents,
						&relative_file_path,
						&mut document_blocks_batch,
						config,
						state.clone(),
					)
					.await?;

					if !document_blocks_batch.is_empty() {
						process_document_blocks_batch(store, &document_blocks_batch, config)
							.await?;
					}
				} else {
					// Handle code files
					let mut code_blocks_batch = Vec::new();
					let mut text_blocks_batch = Vec::new(); // Will remain empty for code files
					let mut all_code_blocks = Vec::new(); // For GraphRAG

					let ctx = ProcessFileContext {
						store,
						config,
						state: state.clone(),
					};
					process_file(
						&ctx,
						&contents,
						&relative_file_path,
						language,
						&mut code_blocks_batch,
						&mut text_blocks_batch,
						&mut all_code_blocks,
					)
					.await?;

					if !code_blocks_batch.is_empty() {
						process_code_blocks_batch(store, &code_blocks_batch, config).await?;
					}
					// No need to process text_blocks_batch since it will be empty for code files

					// Update GraphRAG if enabled and we have new blocks
					if config.graphrag.enabled && !all_code_blocks.is_empty() {
						let graph_builder = graphrag::GraphBuilder::new(config.clone()).await?;
						graph_builder
							.process_code_blocks(&all_code_blocks, Some(state.clone()))
							.await?;
					}
				}

				// Explicitly flush to ensure all data is persisted
				store.flush().await?;
			}
		} else {
			// Handle unsupported file types as chunked text
			// First check if the file extension is in our whitelist
			if is_allowed_text_extension(&absolute_path) {
				if let Ok(contents) = fs::read_to_string(&absolute_path) {
					if is_text_file(&contents) {
						// Ensure we use relative path for storage
						let relative_file_path =
							PathUtils::to_relative_string(&absolute_path, &current_dir);

						let mut text_blocks_batch = Vec::new();
						process_text_file(
							store,
							&contents,
							&relative_file_path,
							&mut text_blocks_batch,
							config,
							state.clone(),
						)
						.await?;

						if !text_blocks_batch.is_empty() {
							process_text_blocks_batch(store, &text_blocks_batch, config).await?;
						}

						// Explicitly flush to ensure all data is persisted
						store.flush().await?;
					}
				}
			}
		}
	}

	Ok(())
}

// Context for file processing to reduce the number of function arguments
struct ProcessFileContext<'a> {
	store: &'a Store,
	config: &'a Config,
	state: SharedState,
}

// Processes a single file, extracting code blocks and adding them to the batch
async fn process_file(
	ctx: &ProcessFileContext<'_>,
	contents: &str,
	file_path: &str,
	language: &str,
	code_blocks_batch: &mut Vec<CodeBlock>,
	_text_blocks_batch: &mut [TextBlock], // Unused for code files - only used for unsupported files
	all_code_blocks: &mut Vec<CodeBlock>,
) -> Result<()> {
	let mut parser = Parser::new();

	// Get force_reindex flag from state
	let force_reindex = ctx.state.read().force_reindex;

	// Get the language implementation
	let lang_impl = match languages::get_language(language) {
		Some(impl_) => impl_,
		None => return Ok(()), // Skip unsupported languages
	};

	// Set the parser language
	parser.set_language(&lang_impl.get_ts_language())?;

	let tree = parser
		.parse(contents, None)
		.unwrap_or_else(|| parser.parse("", None).unwrap());
	let mut code_regions = Vec::new();

	extract_meaningful_regions(
		tree.root_node(),
		contents,
		lang_impl.as_ref(),
		&mut code_regions,
	);

	// Track the number of blocks we added to all_code_blocks for GraphRAG
	let mut graphrag_blocks_added = 0;

	for region in code_regions {
		// Use a hash that's unique to both content and path
		let content_hash = calculate_unique_content_hash(&region.content, file_path);

		// Skip the check if force_reindex is true
		let exists = !force_reindex
			&& ctx
				.store
				.content_exists(&content_hash, "code_blocks")
				.await?;
		if !exists {
			let code_block = CodeBlock {
				path: file_path.to_string(),
				hash: content_hash,
				language: lang_impl.name().to_string(),
				content: region.content.clone(),
				symbols: region.symbols.clone(),
				start_line: region.start_line,
				end_line: region.end_line,
				distance: None, // No relevance score when indexing
			};

			// Add to batch for embedding
			code_blocks_batch.push(code_block.clone());

			// Add to all code blocks for GraphRAG
			if ctx.config.graphrag.enabled {
				all_code_blocks.push(code_block);
				graphrag_blocks_added += 1;
			}
		} else if ctx.config.graphrag.enabled {
			// If skipping because block exists, but we need for GraphRAG, fetch from store
			if let Ok(existing_block) = ctx.store.get_code_block_by_hash(&content_hash).await {
				// Add the existing block to the GraphRAG collection
				all_code_blocks.push(existing_block);
				graphrag_blocks_added += 1;
			}
		}
	}

	// Update GraphRAG state if enabled and blocks were added
	if ctx.config.graphrag.enabled && graphrag_blocks_added > 0 {
		let mut state_guard = ctx.state.write();
		state_guard.graphrag_blocks += graphrag_blocks_added;
	}

	// Note: We DON'T create text blocks for code files - only for unsupported file types
	// Code files are already indexed as semantic code blocks above

	Ok(())
}

/// Represents a meaningful code block/region with tree-sitter node information.
#[derive(Clone)]
struct CodeRegion {
	content: String,
	symbols: Vec<String>,
	start_line: usize,
	end_line: usize,
	node_kind: String, // Store the original tree-sitter node kind
	node_id: usize,    // Store a unique node identifier for grouping
}

// Configuration for single-line block merging
const MAX_LINES_PER_BLOCK: usize = 15; // Maximum lines in a merged block
const MIN_LINES_TO_MERGE: usize = 2; // Minimum consecutive single-lines to merge

/// Recursively extracts meaningful regions based on node kinds.
/// Includes smart merging of single-line declarations.
fn extract_meaningful_regions(
	node: Node,
	contents: &str,
	lang_impl: &dyn languages::Language,
	regions: &mut Vec<CodeRegion>,
) {
	let meaningful_kinds = lang_impl.get_meaningful_kinds();
	let mut candidate_regions = Vec::new();

	// First pass: collect all meaningful regions without merging
	collect_meaningful_regions_recursive(
		node,
		contents,
		lang_impl,
		&meaningful_kinds,
		&mut candidate_regions,
	);

	// Second pass: apply smart merging logic
	apply_smart_merging(candidate_regions, regions, lang_impl);
}

/// Recursively collects meaningful regions without merging
fn collect_meaningful_regions_recursive(
	node: Node,
	contents: &str,
	lang_impl: &dyn languages::Language,
	meaningful_kinds: &[&str],
	regions: &mut Vec<CodeRegion>,
) {
	let node_kind = node.kind();

	if meaningful_kinds.contains(&node_kind) {
		let (combined_content, start_line) = combine_with_preceding_comments(node, contents);
		let end_line = node.end_position().row;
		let symbols = lang_impl.extract_symbols(node, contents);

		// Only create a region if we have meaningful content
		if !combined_content.trim().is_empty() {
			// Ensure we have at least one symbol by using the node kind if necessary
			let mut final_symbols = symbols;
			if final_symbols.is_empty() {
				// Create a default symbol from the node kind
				final_symbols.push(format!("{}_{}", node_kind, start_line));
			}

			regions.push(CodeRegion {
				content: combined_content,
				symbols: final_symbols,
				start_line,
				end_line,
				node_kind: node_kind.to_string(),
				node_id: node.id(),
			});
		}
		return;
	}

	let mut cursor = node.walk();
	if cursor.goto_first_child() {
		loop {
			collect_meaningful_regions_recursive(
				cursor.node(),
				contents,
				lang_impl,
				meaningful_kinds,
				regions,
			);
			if !cursor.goto_next_sibling() {
				break;
			}
		}
	}
}

/// Applies smart merging logic to consolidate single-line declarations
fn apply_smart_merging(
	candidate_regions: Vec<CodeRegion>,
	final_regions: &mut Vec<CodeRegion>,
	lang_impl: &dyn languages::Language,
) {
	if candidate_regions.is_empty() {
		return;
	}

	let mut i = 0;
	while i < candidate_regions.len() {
		let current = &candidate_regions[i];

		// Check if this is a single-line block
		if is_single_line_declaration(current) {
			// Look ahead to find consecutive single-line blocks
			let mut consecutive_single_lines = vec![current];
			let mut j = i + 1;

			while j < candidate_regions.len() {
				let next = &candidate_regions[j];
				if is_single_line_declaration(next)
					&& are_consecutive_or_related(
						consecutive_single_lines.last().unwrap(),
						next,
						lang_impl,
					) {
					consecutive_single_lines.push(next);
					j += 1;
				} else {
					break;
				}
			}

			// If we have enough single-line blocks to merge
			if consecutive_single_lines.len() >= MIN_LINES_TO_MERGE {
				merge_single_line_blocks(consecutive_single_lines, final_regions, lang_impl);
				i = j; // Skip the processed blocks
			} else {
				// Not enough to merge, add as-is
				final_regions.push(current.clone());
				i += 1;
			}
		} else {
			// Multi-line block, add as-is
			final_regions.push(current.clone());
			i += 1;
		}
	}
}

/// Checks if a code region is a single-line declaration
fn is_single_line_declaration(region: &CodeRegion) -> bool {
	// Consider it single-line if it's 1 line or very short content
	let line_count = region.end_line - region.start_line + 1;
	let is_short = region.content.trim().lines().count() <= 1 || region.content.len() < 80;

	line_count <= 1 || (line_count <= 2 && is_short)
}

/// Checks if two single-line regions are consecutive or thematically related
fn are_consecutive_or_related(
	first: &CodeRegion,
	second: &CodeRegion,
	lang_impl: &dyn languages::Language,
) -> bool {
	// Consider consecutive if they're within a few lines of each other
	let line_gap = second.start_line.saturating_sub(first.end_line);

	// Allow small gaps (for comments or empty lines)
	if line_gap <= 3 {
		return true;
	}

	// Check if they're thematically related (same type of declaration)
	if are_thematically_related(first, second, lang_impl) {
		return line_gap <= 10; // Allow larger gaps for related content
	}

	false
}

/// Checks if two regions contain thematically related content using language-specific logic
fn are_thematically_related(
	first: &CodeRegion,
	second: &CodeRegion,
	lang_impl: &dyn languages::Language,
) -> bool {
	// Direct node kind match
	if first.node_kind == second.node_kind {
		return true;
	}

	// Use language-specific semantic equivalence
	lang_impl.are_node_types_equivalent(&first.node_kind, &second.node_kind)
}

/// Merges consecutive single-line blocks into optimally-sized chunks
fn merge_single_line_blocks(
	single_lines: Vec<&CodeRegion>,
	final_regions: &mut Vec<CodeRegion>,
	lang_impl: &dyn languages::Language,
) {
	if single_lines.is_empty() {
		return;
	}

	// Group by theme first
	let mut themed_groups: Vec<Vec<&CodeRegion>> = Vec::new();
	let mut current_group = vec![single_lines[0]];

	for region in single_lines.iter().skip(1) {
		if are_thematically_related(current_group[0], region, lang_impl) {
			current_group.push(region);
		} else {
			themed_groups.push(current_group);
			current_group = vec![region];
		}
	}
	themed_groups.push(current_group);

	// Process each themed group
	for group in themed_groups {
		create_merged_blocks_from_group(group, final_regions, lang_impl);
	}
}

/// Creates merged blocks from a thematically grouped set of single-line regions
fn create_merged_blocks_from_group(
	group: Vec<&CodeRegion>,
	final_regions: &mut Vec<CodeRegion>,
	lang_impl: &dyn languages::Language,
) {
	if group.is_empty() {
		return;
	}

	// Split into chunks if the group is too large
	let chunks: Vec<&[&CodeRegion]> = group.chunks(MAX_LINES_PER_BLOCK).collect();

	for chunk in chunks {
		if chunk.is_empty() {
			continue;
		}

		let start_line = chunk[0].start_line;
		let end_line = chunk.last().unwrap().end_line;

		// Combine content from all regions in the chunk
		let mut combined_content = String::new();
		let mut all_symbols = Vec::new();

		for (i, region) in chunk.iter().enumerate() {
			if i > 0 {
				combined_content.push('\n');
			}
			combined_content.push_str(&region.content);
			all_symbols.extend(region.symbols.clone());
		}

		// Deduplicate symbols
		all_symbols.sort();
		all_symbols.dedup();

		// Add a descriptive comment to the merged block using language-specific description
		let block_type = determine_block_type(chunk[0], lang_impl);
		let block_description =
			format!("// Merged {} ({} declarations)\n", block_type, chunk.len());
		let final_content = block_description + &combined_content;

		final_regions.push(CodeRegion {
			content: final_content,
			symbols: all_symbols,
			start_line,
			end_line,
			node_kind: chunk[0].node_kind.clone(),
			node_id: chunk[0].node_id, // Use the first region's node_id as representative
		});
	}
}

/// Determines the type of block for descriptive purposes using language-specific logic
fn determine_block_type(
	sample_region: &CodeRegion,
	lang_impl: &dyn languages::Language,
) -> &'static str {
	lang_impl.get_node_type_description(&sample_region.node_kind)
}

/// Combines preceding comment or attribute nodes with a declaration node.
fn combine_with_preceding_comments(node: Node, contents: &str) -> (String, usize) {
	let mut combined_start = node.start_position().row;
	let mut snippet = String::new();
	if let Some(parent) = node.parent() {
		let mut cursor = parent.walk();
		let mut preceding = Vec::new();
		for child in parent.children(&mut cursor) {
			if child.id() == node.id() {
				break;
			} else {
				preceding.push(child);
			}
		}
		if let Some(last) = preceding.last() {
			let kind = last.kind();
			if kind.contains("comment") || kind.contains("attribute") {
				combined_start = last.start_position().row;
				snippet.push_str(&contents[last.start_byte()..last.end_byte()]);
				snippet.push('\n');
			}
		}
	}
	snippet.push_str(&contents[node.start_byte()..node.end_byte()]);
	(snippet, combined_start)
}

async fn process_code_blocks_batch(
	store: &Store,
	blocks: &[CodeBlock],
	config: &Config,
) -> Result<()> {
	let start_time = std::time::Instant::now();
	let contents: Vec<String> = blocks.iter().map(|b| b.content.clone()).collect();
	let embeddings = crate::embedding::generate_embeddings_batch(contents, true, config).await?;
	store.store_code_blocks(blocks, embeddings).await?;

	let duration_ms = start_time.elapsed().as_millis() as u64;
	log_performance_metrics("code_blocks_batch", duration_ms, blocks.len(), None);

	Ok(())
}

async fn process_text_blocks_batch(
	store: &Store,
	blocks: &[TextBlock],
	config: &Config,
) -> Result<()> {
	let start_time = std::time::Instant::now();
	let contents: Vec<String> = blocks.iter().map(|b| b.content.clone()).collect();
	let embeddings = crate::embedding::generate_embeddings_batch(contents, false, config).await?;
	store.store_text_blocks(blocks, embeddings).await?;

	let duration_ms = start_time.elapsed().as_millis() as u64;
	log_performance_metrics("text_blocks_batch", duration_ms, blocks.len(), None);

	Ok(())
}

async fn process_document_blocks_batch(
	store: &Store,
	blocks: &[DocumentBlock],
	config: &Config,
) -> Result<()> {
	let start_time = std::time::Instant::now();
	let contents: Vec<String> = blocks.iter().map(|b| b.content.clone()).collect();
	let embeddings = crate::embedding::generate_embeddings_batch(contents, false, config).await?;
	store.store_document_blocks(blocks, embeddings).await?;

	let duration_ms = start_time.elapsed().as_millis() as u64;
	log_performance_metrics("document_blocks_batch", duration_ms, blocks.len(), None);

	Ok(())
}

// Constants for text chunking
const DEFAULT_CHUNK_SIZE: usize = 2000; // characters (increased from 1000)
const DEFAULT_OVERLAP: usize = 200; // characters

// Whitelist of file extensions that we allow for text indexing
const ALLOWED_TEXT_EXTENSIONS: &[&str] = &[
	"txt",
	"log",
	"xml",
	"html",
	"htm",
	"csv",
	"tsv",
	"readme",
	"license",
	"changelog",
	"authors",
	"contributors",
];

/// Check if a file extension is allowed for text indexing
fn is_allowed_text_extension(path: &std::path::Path) -> bool {
	if let Some(extension) = path.extension() {
		if let Some(ext_str) = extension.to_str() {
			return ALLOWED_TEXT_EXTENSIONS.contains(&ext_str.to_lowercase().as_str());
		}
	}

	// Also check for files without extensions that have common text names
	if let Some(file_name) = path.file_name() {
		if let Some(name_str) = file_name.to_str() {
			let name_lower = name_str.to_lowercase();
			return matches!(
				name_lower.as_str(),
				"readme"
					| "license" | "changelog"
					| "authors" | "contributors"
					| "makefile" | "dockerfile"
					| "gitignore" | ".gitignore"
			);
		}
	}

	false
}

/// Check if a file contains readable text
fn is_text_file(contents: &str) -> bool {
	if contents.is_empty() {
		return false;
	}

	// Check for NULL bytes - strong indicator of binary
	if contents.chars().any(|c| c == '\0') {
		return false;
	}

	// If no NULL bytes, check printable character ratio (more Unicode friendly)
	// We use chars().count() for a more accurate count of Unicode characters.
	let total_chars = contents.chars().count();
	if total_chars == 0 {
		// Should be caught by contents.is_empty() but good for safety
		return false;
	}

	// Consider characters as "printable-looking" if they are not control characters,
	// or if they are whitespace (which includes \n, \t, etc.).
	// char::is_control() identifies control characters.
	// char::is_whitespace() is Unicode-aware.
	let printable_looking_chars = contents
		.chars()
		.filter(|&c| !c.is_control() || c.is_whitespace())
		.count();

	let printable_ratio = printable_looking_chars as f64 / total_chars as f64;

	// We can keep the 0.8 threshold, or slightly adjust if needed after testing.
	// This check is now more lenient towards Unicode text.
	printable_ratio > 0.8
}

struct TextChunkWithLines {
	content: String,
	start_line: usize, // 0-indexed
	end_line: usize,   // 0-indexed, inclusive
}

fn chunk_text(content: &str, chunk_size: usize, overlap: usize) -> Vec<TextChunkWithLines> {
	let mut chunks = Vec::new();
	let chars: Vec<char> = content.chars().collect();
	let content_len = chars.len();

	if content_len == 0 {
		return chunks;
	}

	// Pre-calculate line start character offsets (0-indexed)
	// These are the character indices where each line begins.
	let mut line_starts: Vec<usize> = vec![0]; // Line 0 starts at character 0
	for (i, &char_val) in chars.iter().enumerate() {
		if char_val == '\n' && i + 1 < content_len {
			// If there's a character after '\n', it starts a new line
			line_starts.push(i + 1);
		}
	}

	let mut current_char_offset = 0;
	let mut previous_iteration_offset = usize::MAX; // Used to detect stuck loops

	while current_char_offset < content_len {
		// Safety break for stuck loops
		if current_char_offset == previous_iteration_offset {
			// This should not be reached if advancement logic is perfect, but acts as a safeguard.
			// eprintln!("Warning: chunk_text detected no progress, forcing advance. Offset: {}", current_char_offset);
			current_char_offset += 1;
			if current_char_offset >= content_len {
				break;
			}
		}
		previous_iteration_offset = current_char_offset;

		let end_char_offset = std::cmp::min(current_char_offset + chunk_size, content_len);

		let chunk_str: String = chars[current_char_offset..end_char_offset].iter().collect();

		// Determine start_line for the current chunk (0-indexed)
		// partition_point returns the index of the first element `el` for which `predicate(el)` is false.
		// We want the count of lines that start *at or before* current_char_offset.
		let start_line_idx = line_starts
			.partition_point(|&line_start_char_idx| line_start_char_idx <= current_char_offset);
		let start_line = start_line_idx.saturating_sub(1); // Convert count to 0-indexed line number

		// Determine end_line for the current chunk (0-indexed, inclusive)
		// The character at end_char_offset - 1 is the last char in the chunk.
		let last_char_in_chunk_offset = if end_char_offset > current_char_offset {
			// Ensure not an empty chunk
			end_char_offset - 1
		} else {
			current_char_offset
		};

		let end_line_idx = line_starts.partition_point(|&line_start_char_idx| {
			line_start_char_idx <= last_char_in_chunk_offset
		});
		let end_line = end_line_idx.saturating_sub(1);

		chunks.push(TextChunkWithLines {
			content: chunk_str,
			start_line,
			end_line: std::cmp::max(start_line, end_line), // Ensure end_line >= start_line
		});

		if end_char_offset >= content_len {
			break; // Reached the end of content
		}

		// Advance current_char_offset for the next chunk
		let mut next_start_offset = end_char_offset.saturating_sub(overlap);

		// Ensure progress: next_start_offset must be greater than current_char_offset if we are not at the end.
		if next_start_offset <= current_char_offset {
			next_start_offset = current_char_offset + 1;
		}
		current_char_offset = next_start_offset;
	}
	chunks
}

/// Process an unsupported file as chunked text blocks
/// Only processes files with whitelisted extensions to avoid indexing
/// binary files, lock files, and other non-useful content.
/// Supported extensions: txt, log, xml, html, css, sql, csv, yaml, toml, ini, conf, etc.
/// Chunk size: 2000 characters with 200 character overlap.
/// FIXED: Now stores simple path without chunk numbers for better display
async fn process_text_file(
	store: &Store,
	contents: &str,
	file_path: &str,
	text_blocks_batch: &mut Vec<TextBlock>,
	_config: &Config,
	state: SharedState,
) -> Result<()> {
	let force_reindex = state.read().force_reindex;

	// Split content into chunks
	let chunks = chunk_text(contents, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP);

	for (chunk_idx, chunk_with_lines) in chunks.iter().enumerate() {
		// Use chunk index in hash for uniqueness but keep path clean
		let chunk_hash = calculate_unique_content_hash(
			&chunk_with_lines.content,
			&format!("{}#{}", file_path, chunk_idx),
		);

		// Skip the check if force_reindex is true
		let exists = !force_reindex && store.content_exists(&chunk_hash, "text_blocks").await?;
		if !exists {
			text_blocks_batch.push(TextBlock {
				path: file_path.to_string(),
				language: "text".to_string(),
				content: chunk_with_lines.content.clone(),
				start_line: chunk_with_lines.start_line, // Use directly from TextChunkWithLines
				end_line: chunk_with_lines.end_line,     // Use directly from TextChunkWithLines
				hash: chunk_hash,
				distance: None,
			});
		}
	}

	Ok(())
}

// Process a markdown file, extracting document blocks
async fn process_markdown_file(
	store: &Store,
	contents: &str,
	file_path: &str,
	document_blocks_batch: &mut Vec<DocumentBlock>,
	_config: &Config,
	state: SharedState,
) -> Result<()> {
	// Get force_reindex flag from state
	let force_reindex = state.read().force_reindex;

	// Parse markdown content into document blocks
	let document_blocks = parse_markdown_content(contents, file_path);

	for doc_block in document_blocks {
		// Check if this document block already exists (unless force reindex)
		let exists = !force_reindex
			&& store
				.content_exists(&doc_block.hash, "document_blocks")
				.await?;
		if !exists {
			document_blocks_batch.push(doc_block);
		}
	}

	Ok(())
}

// NEW DIFFERENTIAL PROCESSING FUNCTIONS

// Differential processing for code files - only updates changed blocks
async fn process_file_differential(
	ctx: &ProcessFileContext<'_>,
	contents: &str,
	file_path: &str,
	language: &str,
	code_blocks_batch: &mut Vec<CodeBlock>,
	_text_blocks_batch: &mut [TextBlock], // Unused for code files
	all_code_blocks: &mut Vec<CodeBlock>,
) -> Result<()> {
	let mut parser = Parser::new();

	// Get force_reindex flag from state
	let force_reindex = ctx.state.read().force_reindex;

	// Get the language implementation
	let lang_impl = match languages::get_language(language) {
		Some(impl_) => impl_,
		None => return Ok(()), // Skip unsupported languages
	};

	// Set the parser language
	parser.set_language(&lang_impl.get_ts_language())?;

	let tree = parser
		.parse(contents, None)
		.unwrap_or_else(|| parser.parse("", None).unwrap());
	let mut code_regions = Vec::new();

	extract_meaningful_regions(
		tree.root_node(),
		contents,
		lang_impl.as_ref(),
		&mut code_regions,
	);

	// If not force reindexing, get existing hashes for this file to compare
	let existing_hashes = if force_reindex {
		Vec::new()
	} else {
		ctx.store
			.get_file_blocks_metadata(file_path, "code_blocks")
			.await?
	};

	// Create set of new hashes for this file
	let mut new_hashes = std::collections::HashSet::new();
	let mut graphrag_blocks_added = 0;

	for region in code_regions {
		// Use a hash that's unique to both content and path
		let content_hash = calculate_unique_content_hash(&region.content, file_path);
		new_hashes.insert(content_hash.clone());

		// Skip the check if force_reindex is true
		let exists = !force_reindex
			&& ctx
				.store
				.content_exists(&content_hash, "code_blocks")
				.await?;
		if !exists {
			let code_block = CodeBlock {
				path: file_path.to_string(),
				hash: content_hash.clone(),
				language: lang_impl.name().to_string(),
				content: region.content.clone(),
				symbols: region.symbols.clone(),
				start_line: region.start_line,
				end_line: region.end_line,
				distance: None, // No relevance score when indexing
			};

			// Add to batch for embedding
			code_blocks_batch.push(code_block.clone());

			// Add to all code blocks for GraphRAG
			if ctx.config.graphrag.enabled {
				all_code_blocks.push(code_block);
				graphrag_blocks_added += 1;
			}
		} else if ctx.config.graphrag.enabled {
			// If skipping because block exists, but we need for GraphRAG, fetch from store
			if let Ok(existing_block) = ctx.store.get_code_block_by_hash(&content_hash).await {
				// Add the existing block to the GraphRAG collection
				all_code_blocks.push(existing_block);
				graphrag_blocks_added += 1;
			}
		}
	}

	// Remove blocks that no longer exist (only if not force reindexing)
	if !force_reindex && !existing_hashes.is_empty() {
		let hashes_to_remove: Vec<String> = existing_hashes
			.into_iter()
			.filter(|hash| !new_hashes.contains(hash))
			.collect();

		if !hashes_to_remove.is_empty() {
			ctx.store
				.remove_blocks_by_hashes(&hashes_to_remove, "code_blocks")
				.await?;
		}
	}

	// Update GraphRAG state if enabled and blocks were added
	if ctx.config.graphrag.enabled && graphrag_blocks_added > 0 {
		let mut state_guard = ctx.state.write();
		state_guard.graphrag_blocks += graphrag_blocks_added;
	}

	Ok(())
}

// Differential processing for text files - only updates changed blocks
// FIXED: Now stores simple path without chunk numbers
async fn process_text_file_differential(
	store: &Store,
	contents: &str,
	file_path: &str,
	text_blocks_batch: &mut Vec<TextBlock>,
	_config: &Config,
	state: SharedState,
) -> Result<()> {
	let force_reindex = state.read().force_reindex;

	// Get existing text block hashes for this file (including chunked versions)
	let existing_hashes = if force_reindex {
		Vec::new()
	} else {
		// Get blocks for this file path (the chunks will have same path now)
		store
			.get_file_blocks_metadata(file_path, "text_blocks")
			.await?
	};

	// Split content into chunks
	let chunks = chunk_text(contents, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP);
	let mut new_hashes = std::collections::HashSet::new();

	for (chunk_idx, chunk_with_lines) in chunks.iter().enumerate() {
		// Use chunk index in hash for uniqueness but keep path clean
		let chunk_hash = calculate_unique_content_hash(
			&chunk_with_lines.content,
			&format!("{}#{}", file_path, chunk_idx),
		);
		new_hashes.insert(chunk_hash.clone());

		// Skip the check if force_reindex is true
		let exists = !force_reindex && store.content_exists(&chunk_hash, "text_blocks").await?;
		if !exists {
			text_blocks_batch.push(TextBlock {
				path: file_path.to_string(),
				language: "text".to_string(),
				content: chunk_with_lines.content.clone(),
				start_line: chunk_with_lines.start_line, // Use directly from TextChunkWithLines
				end_line: chunk_with_lines.end_line,     // Use directly from TextChunkWithLines
				hash: chunk_hash,
				distance: None,
			});
		}
	}

	// Remove blocks that no longer exist (only if not force reindexing)
	if !force_reindex && !existing_hashes.is_empty() {
		let hashes_to_remove: Vec<String> = existing_hashes
			.into_iter()
			.filter(|hash| !new_hashes.contains(hash))
			.collect();

		if !hashes_to_remove.is_empty() {
			store
				.remove_blocks_by_hashes(&hashes_to_remove, "text_blocks")
				.await?;
		}
	}

	Ok(())
}

// Differential processing for markdown files - only updates changed blocks
async fn process_markdown_file_differential(
	store: &Store,
	contents: &str,
	file_path: &str,
	document_blocks_batch: &mut Vec<DocumentBlock>,
	_config: &Config,
	state: SharedState,
) -> Result<()> {
	// Get force_reindex flag from state
	let force_reindex = state.read().force_reindex;

	// Get existing document block hashes for this file
	let existing_hashes = if force_reindex {
		Vec::new()
	} else {
		store
			.get_file_blocks_metadata(file_path, "document_blocks")
			.await?
	};

	// Parse markdown content into document blocks
	let document_blocks = parse_markdown_content(contents, file_path);
	let mut new_hashes = std::collections::HashSet::new();

	for doc_block in document_blocks {
		new_hashes.insert(doc_block.hash.clone());

		// Check if this document block already exists (unless force reindex)
		let exists = !force_reindex
			&& store
				.content_exists(&doc_block.hash, "document_blocks")
				.await?;
		if !exists {
			document_blocks_batch.push(doc_block);
		}
	}

	// Remove blocks that no longer exist (only if not force reindexing)
	if !force_reindex && !existing_hashes.is_empty() {
		let hashes_to_remove: Vec<String> = existing_hashes
			.into_iter()
			.filter(|hash| !new_hashes.contains(hash))
			.collect();

		if !hashes_to_remove.is_empty() {
			store
				.remove_blocks_by_hashes(&hashes_to_remove, "document_blocks")
				.await?;
		}
	}

	Ok(())
}
