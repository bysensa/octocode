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

//! Rust language implementation for the indexer

use crate::indexer::languages::Language;
use tree_sitter::Node;

pub struct Rust {}

impl Language for Rust {
	fn name(&self) -> &'static str {
		"rust"
	}

	fn get_ts_language(&self) -> tree_sitter::Language {
		tree_sitter_rust::LANGUAGE.into()
	}

	fn get_meaningful_kinds(&self) -> Vec<&'static str> {
		vec![
			"function_item",
			"struct_item",
			"enum_item",
			// Removed: "impl_item" - can be very large, not semantic
			// Individual functions inside impl blocks will be captured separately
			"trait_item",
			"mod_item",
			"const_item",
			"macro_definition",
		]
	}

	fn extract_symbols(&self, node: Node, contents: &str) -> Vec<String> {
		let mut symbols = Vec::new();

		match node.kind() {
			"function_item" => {
				for child in node.children(&mut node.walk()) {
					if child.kind() == "identifier" {
						if let Ok(n) = child.utf8_text(contents.as_bytes()) {
							symbols.push(n.to_string());
						}
						break;
					}
				}
			}
			"struct_item" | "enum_item" | "trait_item" | "mod_item" | "const_item"
			| "macro_definition" => {
				for child in node.children(&mut node.walk()) {
					if child.kind() == "identifier" || child.kind().contains("name") {
						if let Ok(n) = child.utf8_text(contents.as_bytes()) {
							symbols.push(n.to_string());
						}
						break;
					}
				}
			}

			_ => self.extract_identifiers(node, contents, &mut symbols),
		}

		// Deduplicate symbols before returning
		symbols.sort();
		symbols.dedup();

		symbols
	}

	fn extract_identifiers(&self, node: Node, contents: &str, symbols: &mut Vec<String>) {
		let kind = node.kind();
		// Check if this is a valid identifier and not a property identifier
		if kind.contains("identifier") || kind.contains("name") {
			if let Ok(text) = node.utf8_text(contents.as_bytes()) {
				let t = text.trim();
				if !t.is_empty() && !symbols.contains(&t.to_string()) {
					symbols.push(t.to_string());
				}
			}
		}

		// Continue with recursive traversal
		let mut cursor = node.walk();
		if cursor.goto_first_child() {
			loop {
				self.extract_identifiers(cursor.node(), contents, symbols);
				if !cursor.goto_next_sibling() {
					break;
				}
			}
		}
	}

	fn are_node_types_equivalent(&self, type1: &str, type2: &str) -> bool {
		// Direct match
		if type1 == type2 {
			return true;
		}

		// Rust-specific semantic groups
		let semantic_groups = [
			// Module related
			&["mod_item", "use_declaration", "extern_crate_item"] as &[&str],
			// Type definitions
			&["struct_item", "enum_item", "union_item", "type_item"],
			// Functions
			&["function_item"],
			// Constants and statics
			&["const_item", "static_item"],
			// Traits and implementations
			&["trait_item", "impl_item"],
			// Macros
			&["macro_definition", "macro_rules"],
		];

		// Check if both types belong to the same semantic group
		for group in &semantic_groups {
			let contains_type1 = group.contains(&type1);
			let contains_type2 = group.contains(&type2);

			if contains_type1 && contains_type2 {
				return true;
			}
		}

		false
	}

	fn get_node_type_description(&self, node_type: &str) -> &'static str {
		match node_type {
			"mod_item" => "module declarations",
			"use_declaration" | "extern_crate_item" => "import statements",
			"struct_item" | "enum_item" | "union_item" => "type definitions",
			"type_item" => "type declarations",
			"function_item" => "function declarations",
			"const_item" | "static_item" => "constant declarations",
			"trait_item" => "trait declarations",
			"impl_item" => "implementation blocks",
			"macro_definition" | "macro_rules" => "macro definitions",
			_ => "declarations",
		}
	}

	fn extract_imports_exports(&self, node: Node, contents: &str) -> (Vec<String>, Vec<String>) {
		let mut imports = Vec::new();
		let mut exports = Vec::new();

		match node.kind() {
			"use_declaration" => {
				// Extract use statement for GraphRAG import detection
				if let Ok(use_text) = node.utf8_text(contents.as_bytes()) {
					if let Some(imported_item) = parse_rust_use_statement(use_text) {
						imports.push(imported_item);
					}
				}
			}
			"function_item" | "struct_item" | "enum_item" | "trait_item" | "mod_item"
			| "const_item" | "macro_definition" => {
				// Check if this item is public (exported)
				let mut cursor = node.walk();
				for child in node.children(&mut cursor) {
					if child.kind() == "visibility_modifier" {
						if let Ok(vis_text) = child.utf8_text(contents.as_bytes()) {
							if vis_text.contains("pub") {
								// Extract the item name as an export
								for name_child in node.children(&mut node.walk()) {
									if name_child.kind() == "identifier" {
										if let Ok(name) = name_child.utf8_text(contents.as_bytes())
										{
											exports.push(name.to_string());
											break;
										}
									}
								}
							}
						}
						break;
					}
				}
			}
			_ => {}
		}

		(imports, exports)
	}

	fn resolve_import(
		&self,
		import_path: &str,
		source_file: &str,
		all_files: &[String],
	) -> Option<String> {
		use super::resolution_utils::FileRegistry;

		let registry = FileRegistry::new(all_files);
		let rust_files = registry.get_files_with_extensions(&self.get_file_extensions());

		// Handle different Rust import patterns
		if import_path.starts_with("crate::") {
			// Absolute crate path: crate::module::Item
			let module_path = import_path.strip_prefix("crate::")?;
			self.resolve_crate_import(module_path, source_file, &rust_files)
		} else if import_path.starts_with("super::") {
			// Parent module: super::module::Item
			let module_path = import_path.strip_prefix("super::")?;
			self.resolve_super_import(module_path, source_file, &rust_files)
		} else if import_path.starts_with("self::") {
			// Current module: self::module::Item
			let module_path = import_path.strip_prefix("self::")?;
			self.resolve_self_import(module_path, source_file, &rust_files)
		} else if import_path.contains("::") {
			// External crate or absolute path
			self.resolve_crate_import(import_path, source_file, &rust_files)
		} else {
			// Simple import - look for file in same directory
			self.resolve_simple_import(import_path, source_file, &rust_files)
		}
	}

	fn get_file_extensions(&self) -> Vec<&'static str> {
		vec!["rs"]
	}
}

// Helper function to parse Rust use statements
fn parse_rust_use_statement(use_text: &str) -> Option<String> {
	// Remove "use " prefix and trailing semicolon
	let cleaned = use_text
		.trim()
		.strip_prefix("use ")?
		.trim_end_matches(';')
		.trim();

	// Handle different use patterns:
	// use std::collections::HashMap; -> HashMap
	// use crate::module::Item; -> Item
	// use super::Item; -> Item
	// use self::Item; -> Item
	// use crate::module::{Item1, Item2}; -> Item1 (first item for simplicity)

	if cleaned.contains('{') {
		// Handle grouped imports: use module::{Item1, Item2};
		if let Some(brace_start) = cleaned.find('{') {
			if let Some(brace_end) = cleaned.find('}') {
				let items = &cleaned[brace_start + 1..brace_end];
				// Take first item for simplicity
				return items.split(',').next().map(|s| s.trim().to_string());
			}
		}
	}

	// Handle simple imports: use module::Item;
	if let Some(last_part) = cleaned.split("::").last() {
		Some(last_part.trim().to_string())
	} else {
		Some(cleaned.to_string())
	}
}

impl Rust {
	/// Resolve crate-relative imports like crate::module::Item
	fn resolve_crate_import(
		&self,
		module_path: &str,
		source_file: &str,
		rust_files: &[String],
	) -> Option<String> {
		let parts: Vec<&str> = module_path.split("::").collect();
		if parts.is_empty() {
			return None;
		}

		// Find crate root
		let crate_root = self.find_crate_root(source_file, rust_files)?;
		let crate_dir = std::path::Path::new(&crate_root).parent()?;

		// Build path from module parts
		let mut current_path = crate_dir.to_path_buf();
		for (i, part) in parts.iter().enumerate() {
			if i == parts.len() - 1 {
				// Last part could be a file or module
				let file_path = current_path.join(format!("{}.rs", part));
				let file_path_str = file_path.to_string_lossy().to_string();
				if rust_files.iter().any(|f| f == &file_path_str) {
					return Some(file_path_str);
				}

				let mod_path = current_path.join(part).join("mod.rs");
				let mod_path_str = mod_path.to_string_lossy().to_string();
				if rust_files.iter().any(|f| f == &mod_path_str) {
					return Some(mod_path_str);
				}
			} else {
				current_path = current_path.join(part);
			}
		}

		None
	}

	/// Resolve super:: imports (parent module)
	fn resolve_super_import(
		&self,
		module_path: &str,
		source_file: &str,
		rust_files: &[String],
	) -> Option<String> {
		let source_path = std::path::Path::new(source_file);
		let parent_dir = source_path.parent()?.parent()?;

		self.resolve_relative_import(module_path, parent_dir, rust_files)
	}

	/// Resolve self:: imports (current module)
	fn resolve_self_import(
		&self,
		module_path: &str,
		source_file: &str,
		rust_files: &[String],
	) -> Option<String> {
		let source_path = std::path::Path::new(source_file);
		let current_dir = source_path.parent()?;

		self.resolve_relative_import(module_path, current_dir, rust_files)
	}

	/// Resolve simple imports in same directory
	fn resolve_simple_import(
		&self,
		import_path: &str,
		source_file: &str,
		rust_files: &[String],
	) -> Option<String> {
		let source_path = std::path::Path::new(source_file);
		let source_dir = source_path.parent()?;
		let target_file = source_dir.join(format!("{}.rs", import_path));
		let target_str = target_file.to_string_lossy().to_string();

		if rust_files.iter().any(|f| f == &target_str) {
			Some(target_str)
		} else {
			None
		}
	}

	/// Resolve relative imports from a base directory
	fn resolve_relative_import(
		&self,
		module_path: &str,
		base_dir: &std::path::Path,
		rust_files: &[String],
	) -> Option<String> {
		let parts: Vec<&str> = module_path.split("::").collect();
		if parts.is_empty() {
			return None;
		}

		let mut current_path = base_dir.to_path_buf();
		for (i, part) in parts.iter().enumerate() {
			if i == parts.len() - 1 {
				// Last part - look for file
				let file_path = current_path.join(format!("{}.rs", part));
				let file_path_str = file_path.to_string_lossy().to_string();
				if rust_files.iter().any(|f| f == &file_path_str) {
					return Some(file_path_str);
				}
			} else {
				current_path = current_path.join(part);
			}
		}

		None
	}

	/// Find the crate root (lib.rs or main.rs)
	fn find_crate_root(&self, source_file: &str, rust_files: &[String]) -> Option<String> {
		let source_path = std::path::Path::new(source_file);
		let mut current_dir = source_path.parent()?;

		loop {
			// Look for lib.rs or main.rs
			let lib_path = current_dir.join("lib.rs");
			let lib_path_str = lib_path.to_string_lossy().to_string();
			if rust_files.iter().any(|f| f == &lib_path_str) {
				return Some(lib_path_str);
			}

			let main_path = current_dir.join("main.rs");
			let main_path_str = main_path.to_string_lossy().to_string();
			if rust_files.iter().any(|f| f == &main_path_str) {
				return Some(main_path_str);
			}

			// Move up one directory
			current_dir = current_dir.parent()?;
		}
	}
}
