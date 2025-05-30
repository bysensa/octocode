// Octocode - Intelligent Code Indexer and Graph Builder
// Copyright (c) 2025 Muvon Un Limited
// Licensed under the MIT License

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{anyhow, Context, Result};
use clap::Args;
use walkdir::WalkDir;

#[derive(Args)]
pub struct FormatArgs {
	/// Apply formatting to files (dry-run by default)
	#[arg(short, long)]
	pub apply: bool,

	/// Commit changes after formatting with a standard message
	#[arg(short, long)]
	pub commit: bool,

	/// Custom commit message (only used with --commit)
	#[arg(short = 'm', long)]
	pub message: Option<String>,

	/// Specific files to format (default: all source files)
	pub files: Vec<PathBuf>,

	/// Show verbose output
	#[arg(short, long)]
	pub verbose: bool,
}

pub async fn execute(format_args: &FormatArgs) -> Result<()> {
	let git_root = find_git_root()
		.context("Failed to find git repository root. Make sure you're in a git repository.")?;

	let editorconfig_path = git_root.join(".editorconfig");

	if !editorconfig_path.exists() {
		return Err(anyhow!(".editorconfig file not found in git root: {}", git_root.display()));
	}

	if format_args.verbose {
		println!("Found .editorconfig at: {}", editorconfig_path.display());
		println!("Git root: {}", git_root.display());
	}

	let files_to_format = if format_args.files.is_empty() {
		find_source_files(&git_root)?
	} else {
		format_args.files.clone()
	};

	if files_to_format.is_empty() {
		println!("No files to format found.");
		return Ok(());
	}

	let mut formatted_files = Vec::new();
	let mut total_changes = 0;

	for file_path in &files_to_format {
		if !file_path.exists() {
			if format_args.verbose {
				println!("Skipping non-existent file: {}", file_path.display());
			}
			continue;
		}

		if format_args.verbose {
			println!("Processing: {}", file_path.display());
		}

		let changes = format_file(file_path, format_args.apply, format_args.verbose)
			.with_context(|| format!("Failed to format file: {}", file_path.display()))?;

		if changes > 0 {
			formatted_files.push(file_path.clone());
			total_changes += changes;
		}
	}

	if total_changes == 0 {
		println!("No formatting changes needed.");
		return Ok(());
	}

	println!(
		"Formatting complete: {} changes across {} files{}",
		total_changes,
		formatted_files.len(),
		if format_args.apply { " (applied)" } else { " (dry-run)" }
	);

	if format_args.apply && format_args.commit {
		commit_changes(&formatted_files, format_args.message.as_deref())?;
	}

	Ok(())
}

fn find_git_root() -> Result<PathBuf> {
	let current_dir = std::env::current_dir()
		.context("Failed to get current directory")?;

	let mut path = current_dir.as_path();

	loop {
		if path.join(".git").exists() {
			return Ok(path.to_path_buf());
		}

		match path.parent() {
			Some(parent) => path = parent,
			None => return Err(anyhow!("Not in a git repository")),
		}
	}
}

fn find_source_files(root: &Path) -> Result<Vec<PathBuf>> {
	let mut files = Vec::new();

	// Common source file extensions
	let source_extensions = [
		"rs", "py", "js", "ts", "go", "java", "cpp", "c", "h", "hpp",
		"php", "rb", "sh", "bash", "yml", "yaml", "toml", "json",
		"md", "txt", "html", "css", "scss", "sql"
	];

	for entry in WalkDir::new(root)
		.follow_links(false)
		.into_iter()
		.filter_map(|e| e.ok())
	{
		let path = entry.path();

		// Skip hidden directories and files
		if path.components().any(|c| {
			c.as_os_str().to_string_lossy().starts_with('.')
		}) {
			continue;
		}

		// Skip common build/dependency directories
		if path.components().any(|c| {
			matches!(c.as_os_str().to_string_lossy().as_ref(),
				"target" | "node_modules" | "dist" | "build" |
				"__pycache__" | ".git" | ".svn" | ".hg")
		}) {
			continue;
		}

		if path.is_file() {
			if let Some(extension) = path.extension() {
				let ext = extension.to_string_lossy().to_lowercase();
				if source_extensions.contains(&ext.as_str()) {
					files.push(path.to_path_buf());
				}
			}
		}
	}

	Ok(files)
}

fn format_file(
	file_path: &Path,
	apply: bool,
	verbose: bool,
) -> Result<usize> {
	let content = fs::read_to_string(file_path)
		.with_context(|| format!("Failed to read file: {}", file_path.display()))?;

	// Get EditorConfig properties for this file
	let properties = editorconfig::get_config(file_path)
		.map_err(|e| anyhow!("Failed to get editorconfig properties for {}: {}", file_path.display(), e))?;

	let mut new_content = content.clone();
	let mut changes = 0;

	// Apply EditorConfig rules

	// 1. Handle line endings
	if let Some(end_of_line) = properties.get("end_of_line") {
		let target_ending = match end_of_line.as_str() {
			"lf" => "\n",
			"crlf" => "\r\n",
			"cr" => "\r",
			_ => "\n", // default
		};

		// Normalize all line endings to \n first, then apply target
		let normalized = new_content.replace("\r\n", "\n").replace('\r', "\n");
		if target_ending != "\n" {
			new_content = normalized.replace('\n', target_ending);
		} else {
			new_content = normalized;
		}

		if new_content != content {
			changes += 1;
			if verbose {
				println!("  - Fixed line endings");
			}
		}
	}

	// 2. Handle charset (for now, just ensure UTF-8)
	if let Some(charset) = properties.get("charset") {
		if charset == "utf-8" {
			// Content is already read as UTF-8, so this is mainly a validation
			if verbose {
				println!("  - Verified UTF-8 encoding");
			}
		}
	}

	// 3. Handle trailing whitespace
	if let Some(trim_trailing) = properties.get("trim_trailing_whitespace") {
		if trim_trailing == "true" {
			let lines: Vec<&str> = new_content.lines().collect();
			let trimmed_lines: Vec<String> = lines.iter()
				.map(|line| line.trim_end().to_string())
				.collect();

			let line_ending = if new_content.contains("\r\n") {
				"\r\n"
			} else if new_content.contains('\r') {
				"\r"
			} else {
				"\n"
			};

			let trimmed_content = trimmed_lines.join(line_ending);

			// Preserve final newline status
			let ends_with_newline = new_content.ends_with('\n') || new_content.ends_with('\r');
			let mut final_content = trimmed_content;
			if ends_with_newline && !final_content.ends_with('\n') && !final_content.ends_with('\r') {
				final_content.push_str(line_ending);
			}

			if final_content != new_content {
				new_content = final_content;
				changes += 1;
				if verbose {
					println!("  - Trimmed trailing whitespace");
				}
			}
		}
	}

	// 4. Handle final newline
	if let Some(insert_final_newline) = properties.get("insert_final_newline") {
		if insert_final_newline == "true" {
			if !new_content.is_empty() && !new_content.ends_with('\n') && !new_content.ends_with('\r') {
				let line_ending = if new_content.contains("\r\n") {
					"\r\n"
				} else if new_content.contains('\r') {
					"\r"
				} else {
					"\n"
				};
				new_content.push_str(line_ending);
				changes += 1;
				if verbose {
					println!("  - Added final newline");
				}
			}
		} else if insert_final_newline == "false" {
			if new_content.ends_with('\n') || new_content.ends_with('\r') {
				new_content = new_content.trim_end_matches(&['\n', '\r'][..]).to_string();
				changes += 1;
				if verbose {
					println!("  - Removed final newline");
				}
			}
		}
	}

	// 5. Handle indentation (basic conversion)
	if let (Some(indent_style), Some(indent_size_str)) = (properties.get("indent_style"), properties.get("indent_size")) {
		if let Ok(indent_size) = indent_size_str.parse::<usize>() {
			let lines: Vec<&str> = new_content.lines().collect();
			let mut converted_lines = Vec::new();
			let mut indent_changes = 0;

			for line in lines {
				if line.trim().is_empty() {
					converted_lines.push(line.to_string());
					continue;
				}

				let (leading_whitespace, rest) = split_leading_whitespace(line);
				let converted_indent = convert_indentation(
					&leading_whitespace,
					indent_style,
					indent_size,
				);

				if converted_indent != leading_whitespace {
					indent_changes += 1;
				}

				converted_lines.push(format!("{}{}", converted_indent, rest));
			}

			if indent_changes > 0 {
				let line_ending = if new_content.contains("\r\n") {
					"\r\n"
				} else if new_content.contains('\r') {
					"\r"
				} else {
					"\n"
				};

				new_content = converted_lines.join(line_ending);

				// Preserve final newline status
				if content.ends_with('\n') || content.ends_with('\r') {
					if !new_content.ends_with('\n') && !new_content.ends_with('\r') {
						new_content.push_str(line_ending);
					}
				}

				changes += 1;
				if verbose {
					println!("  - Converted indentation ({} lines)", indent_changes);
				}
			}
		}
	}

	// Apply changes if requested
	if apply && changes > 0 {
		fs::write(file_path, &new_content)
			.with_context(|| format!("Failed to write file: {}", file_path.display()))?;
	}

	Ok(changes)
}

fn split_leading_whitespace(line: &str) -> (String, &str) {
	let trimmed = line.trim_start();
	let leading_len = line.len() - trimmed.len();
	(line[..leading_len].to_string(), trimmed)
}

fn convert_indentation(
	whitespace: &str,
	target_style: &str,
	target_size: usize,
) -> String {
	// Convert existing indentation to logical level
	let logical_level = match target_style {
		"tab" => {
			// When converting to tabs, calculate spaces equivalent
			whitespace.chars().fold(0, |acc, c| {
				match c {
					'\t' => acc + target_size,
					' ' => acc + 1,
					_ => acc,
				}
			}) / target_size
		}
		"space" => {
			// When converting to spaces, calculate tab equivalent
			whitespace.chars().fold(0, |acc, c| {
				match c {
					'\t' => acc + target_size,
					' ' => acc + 1,
					_ => acc,
				}
			}) / target_size
		}
		_ => return whitespace.to_string(), // Unknown style, keep as is
	};

	// Generate new indentation
	match target_style {
		"tab" => "\t".repeat(logical_level),
		"space" => " ".repeat(logical_level * target_size),
		_ => whitespace.to_string(), // Unknown style, keep as is
	}
}

fn commit_changes(files: &[PathBuf], custom_message: Option<&str>) -> Result<()> {
	// Add files to git
	for file in files {
		let output = Command::new("git")
			.args(["add", &file.to_string_lossy()])
			.output()
			.context("Failed to execute git add command")?;

		if !output.status.success() {
			return Err(anyhow!(
				"Git add failed for {}: {}",
				file.display(),
				String::from_utf8_lossy(&output.stderr)
			));
		}
	}

	// Create commit message
	let commit_message = custom_message.unwrap_or("Format code according to .editorconfig");

	// Commit changes
	let output = Command::new("git")
		.args(["commit", "-m", commit_message])
		.output()
		.context("Failed to execute git commit command")?;

	if !output.status.success() {
		return Err(anyhow!(
			"Git commit failed: {}",
			String::from_utf8_lossy(&output.stderr)
		));
	}

	println!("Successfully committed formatting changes:");
	println!("  Message: {}", commit_message);
	println!("  Files: {}", files.len());

	Ok(())
}
