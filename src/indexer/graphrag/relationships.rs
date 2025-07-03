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

// GraphRAG relationship discovery logic

use crate::indexer::graphrag::types::{CodeNode, CodeRelationship, FunctionInfo};
use crate::indexer::graphrag::utils::{is_parent_child_relationship, symbols_match};
use crate::store::CodeBlock;
use anyhow::Result;
use std::path::Path;

pub struct RelationshipDiscovery;

impl RelationshipDiscovery {
	// Discover relationships efficiently without AI for most cases
	pub async fn discover_relationships_efficiently(
		new_files: &[CodeNode],
		all_nodes: &[CodeNode],
	) -> Result<Vec<CodeRelationship>> {
		let mut relationships = Vec::new();

		for source_file in new_files {
			// 1. Import/Export relationships (high confidence)
			for import in &source_file.imports {
				for target_file in all_nodes {
					if target_file.id == source_file.id {
						continue;
					}

					// Check if target exports what source imports
					if target_file
						.exports
						.iter()
						.any(|exp| symbols_match(import, exp))
						|| target_file
							.symbols
							.iter()
							.any(|sym| symbols_match(import, sym))
					{
						relationships.push(CodeRelationship {
							source: source_file.id.clone(),
							target: target_file.id.clone(),
							relation_type: "imports".to_string(),
							description: format!("Imports {} from {}", import, target_file.name),
							confidence: 0.9,
							weight: 1.0,
						});
					}
				}
			}

			// 2. Directory-based relationships (medium confidence)
			let source_dir = Path::new(&source_file.path)
				.parent()
				.map(|p| p.to_string_lossy().to_string())
				.unwrap_or_else(|| ".".to_string());

			for other_file in all_nodes {
				if other_file.id == source_file.id {
					continue;
				}

				let other_dir = Path::new(&other_file.path)
					.parent()
					.map(|p| p.to_string_lossy().to_string())
					.unwrap_or_else(|| ".".to_string());

				// Same directory relationship
				if source_dir == other_dir && source_file.language == other_file.language {
					relationships.push(CodeRelationship {
						source: source_file.id.clone(),
						target: other_file.id.clone(),
						relation_type: "sibling_module".to_string(),
						description: format!("Same directory: {}", source_dir),
						confidence: 0.6,
						weight: 0.5,
					});
				}
			}

			// 3. Hierarchical module relationships (high confidence)
			for other_file in all_nodes {
				if other_file.id == source_file.id {
					continue;
				}

				// Check for parent-child relationships based on path structure
				if is_parent_child_relationship(&source_file.path, &other_file.path) {
					let (parent, child) = if source_file.path.len() < other_file.path.len() {
						(&source_file.id, &other_file.id)
					} else {
						(&other_file.id, &source_file.id)
					};

					relationships.push(CodeRelationship {
						source: parent.clone(),
						target: child.clone(),
						relation_type: "contains".to_string(),
						description: "Hierarchical module relationship".to_string(),
						confidence: 0.8,
						weight: 0.7,
					});
				}
			}

			// 4. Language-specific pattern relationships
			Self::discover_language_specific_relationships(
				source_file,
				all_nodes,
				&mut relationships,
			);
		}

		// Deduplicate relationships
		relationships.sort_by(|a, b| {
			(a.source.clone(), a.target.clone(), a.relation_type.clone()).cmp(&(
				b.source.clone(),
				b.target.clone(),
				b.relation_type.clone(),
			))
		});
		relationships.dedup_by(|a, b| {
			a.source == b.source && a.target == b.target && a.relation_type == b.relation_type
		});

		Ok(relationships)
	}

	// Discover language-specific relationships
	fn discover_language_specific_relationships(
		source_file: &CodeNode,
		all_nodes: &[CodeNode],
		relationships: &mut Vec<CodeRelationship>,
	) {
		match source_file.language.as_str() {
			"rust" => {
				Self::discover_rust_relationships(source_file, all_nodes, relationships);
			}
			"javascript" | "typescript" => {
				Self::discover_js_ts_relationships(source_file, all_nodes, relationships);
			}
			"python" => {
				Self::discover_python_relationships(source_file, all_nodes, relationships);
			}
			_ => {
				// Generic patterns for other languages
			}
		}
	}

	// Rust-specific relationship patterns
	fn discover_rust_relationships(
		source_file: &CodeNode,
		all_nodes: &[CodeNode],
		relationships: &mut Vec<CodeRelationship>,
	) {
		for other_file in all_nodes {
			if other_file.id == source_file.id || other_file.language != "rust" {
				continue;
			}

			// Check for mod.rs patterns
			if source_file.name == "mod"
				&& other_file
					.path
					.starts_with(&source_file.path.replace("/mod.rs", "/"))
			{
				relationships.push(CodeRelationship {
					source: source_file.id.clone(),
					target: other_file.id.clone(),
					relation_type: "mod_declaration".to_string(),
					description: "Rust module declaration".to_string(),
					confidence: 0.8,
					weight: 0.8,
				});
			}

			// Check for lib.rs patterns
			if source_file.name == "lib" || source_file.name == "main" {
				let source_dir = Path::new(&source_file.path)
					.parent()
					.map(|p| p.to_string_lossy().to_string())
					.unwrap_or_default();
				if other_file.path.starts_with(&source_dir) {
					relationships.push(CodeRelationship {
						source: source_file.id.clone(),
						target: other_file.id.clone(),
						relation_type: "crate_root".to_string(),
						description: "Rust crate root relationship".to_string(),
						confidence: 0.7,
						weight: 0.6,
					});
				}
			}
		}
	}

	// JavaScript/TypeScript-specific relationship patterns
	fn discover_js_ts_relationships(
		source_file: &CodeNode,
		all_nodes: &[CodeNode],
		relationships: &mut Vec<CodeRelationship>,
	) {
		for other_file in all_nodes {
			if other_file.id == source_file.id
				|| !["javascript", "typescript"].contains(&other_file.language.as_str())
			{
				continue;
			}

			// Check for index.js patterns
			if source_file.name == "index" {
				let source_dir = Path::new(&source_file.path)
					.parent()
					.map(|p| p.to_string_lossy().to_string())
					.unwrap_or_default();
				if other_file.path.starts_with(&source_dir) && other_file.name != "index" {
					relationships.push(CodeRelationship {
						source: source_file.id.clone(),
						target: other_file.id.clone(),
						relation_type: "index_module".to_string(),
						description: "JavaScript index module relationship".to_string(),
						confidence: 0.7,
						weight: 0.6,
					});
				}
			}
		}
	}

	// Python-specific relationship patterns
	fn discover_python_relationships(
		source_file: &CodeNode,
		all_nodes: &[CodeNode],
		relationships: &mut Vec<CodeRelationship>,
	) {
		for other_file in all_nodes {
			if other_file.id == source_file.id || other_file.language != "python" {
				continue;
			}

			// Check for __init__.py patterns
			if source_file.name == "__init__" {
				let source_dir = Path::new(&source_file.path)
					.parent()
					.map(|p| p.to_string_lossy().to_string())
					.unwrap_or_default();
				if other_file.path.starts_with(&source_dir) && other_file.name != "__init__" {
					relationships.push(CodeRelationship {
						source: source_file.id.clone(),
						target: other_file.id.clone(),
						relation_type: "package_init".to_string(),
						description: "Python package initialization".to_string(),
						confidence: 0.8,
						weight: 0.7,
					});
				}
			}
		}
	}

	// Extract function information from a code block efficiently
	pub fn extract_functions_from_block(block: &CodeBlock) -> Result<Vec<FunctionInfo>> {
		let mut functions = Vec::new();

		// Look for function patterns in symbols
		for symbol in &block.symbols {
			if symbol.contains("function_") || symbol.contains("method_") {
				// Parse the symbol to extract function info
				if let Some(function_info) = Self::parse_function_symbol(symbol, block) {
					functions.push(function_info);
				}
			}
		}

		Ok(functions)
	}

	// Parse function symbol to create FunctionInfo
	fn parse_function_symbol(symbol: &str, block: &CodeBlock) -> Option<FunctionInfo> {
		// Simple pattern matching for common function symbol formats
		// This can be expanded based on your language implementations

		symbol
			.strip_prefix("function_")
			.map(|function_name| FunctionInfo {
				name: function_name.to_string(),
				signature: format!("{}(...)", function_name), // Simplified
				start_line: block.start_line as u32,
				end_line: block.end_line as u32,
				calls: Vec::new(), // Will be populated during relationship discovery
				called_by: Vec::new(),
				parameters: Vec::new(), // Could be extracted from content if needed
				return_type: None,
			})
	}

	// Extract imports/exports efficiently based on language patterns and symbols
	pub fn extract_imports_exports_efficient(
		symbols: &[String],
		_language: &str,
		_relative_path: &str,
	) -> (Vec<String>, Vec<String>) {
		// This function is now deprecated in favor of language-specific extraction
		// during AST parsing. For backward compatibility, treat all symbols as exports
		let mut exports = Vec::new();

		for symbol in symbols {
			if !symbol.is_empty() && !symbol.starts_with("IMPORT:") {
				exports.push(symbol.clone());
			}
		}

		// Return empty imports since real import extraction happens at AST level
		(Vec::new(), exports)
	}
	// Determine file kind based on path patterns
	// Determine file kind based on path patterns
	pub fn determine_file_kind(relative_path: &str) -> String {
		if relative_path.contains("/src/") || relative_path.contains("/lib/") {
			"source_file".to_string()
		} else if relative_path.contains("/test")
			|| relative_path.contains("_test.")
			|| relative_path.contains(".test.")
		{
			"test_file".to_string()
		} else if relative_path.ends_with(".md")
			|| relative_path.ends_with(".txt")
			|| relative_path.ends_with(".rst")
		{
			"documentation".to_string()
		} else if relative_path.contains("/config") || relative_path.contains(".config") {
			"config_file".to_string()
		} else if relative_path.contains("/examples") || relative_path.contains("/demo") {
			"example_file".to_string()
		} else {
			"file".to_string()
		}
	}

	// Generate simple description without AI for speed (fallback and default)
	pub fn generate_simple_description(
		file_name: &str,
		language: &str,
		symbols: &[String],
		lines: u32,
	) -> String {
		let function_count = symbols
			.iter()
			.filter(|s| s.contains("function_") || s.contains("method_"))
			.count();
		let class_count = symbols
			.iter()
			.filter(|s| s.contains("class_") || s.contains("struct_"))
			.count();

		if function_count > 0 && class_count > 0 {
			format!(
				"{} {} file with {} functions and {} classes ({} lines)",
				file_name, language, function_count, class_count, lines
			)
		} else if function_count > 0 {
			format!(
				"{} {} file with {} functions ({} lines)",
				file_name, language, function_count, lines
			)
		} else if class_count > 0 {
			format!(
				"{} {} file with {} classes ({} lines)",
				file_name, language, class_count, lines
			)
		} else {
			format!("{} {} file ({} lines)", file_name, language, lines)
		}
	}
}
