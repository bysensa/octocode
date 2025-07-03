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

//! PHP language implementation for the indexer

use crate::indexer::languages::Language;
use tree_sitter::Node;

pub struct Php {}

impl Language for Php {
	fn name(&self) -> &'static str {
		"php"
	}

	fn get_ts_language(&self) -> tree_sitter::Language {
		tree_sitter_php::LANGUAGE_PHP.into()
	}

	fn get_meaningful_kinds(&self) -> Vec<&'static str> {
		vec![
			"function_definition",
			"method_declaration",
			"class_declaration",
			"namespace_definition",
			"namespace_use_declaration",
			// Removed: "trait_declaration" - too large, not semantic
			// Removed: "interface_declaration" - too large, not semantic
		]
	}

	fn extract_symbols(&self, node: Node, contents: &str) -> Vec<String> {
		let mut symbols = Vec::new();

		match node.kind() {
			"function_definition" | "method_declaration" => {
				// Extract the name of the function or method
				for child in node.children(&mut node.walk()) {
					if child.kind() == "name" {
						if let Ok(name) = child.utf8_text(contents.as_bytes()) {
							symbols.push(name.to_string());
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
		// Check if this is a valid identifier or name
		if kind == "name" || kind == "variable_name" {
			if let Ok(text) = node.utf8_text(contents.as_bytes()) {
				let t = text.trim();
				// For PHP variables, remove the $ prefix
				let t = if let Some(stripped) = t.strip_prefix('$') {
					stripped
				} else {
					t
				};

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

		// PHP-specific semantic groups
		let semantic_groups = [
			// Functions and methods
			&["function_definition", "method_declaration"] as &[&str],
			// Class-related declarations
			&[
				"class_declaration",
				"trait_declaration",
				"interface_declaration",
			],
			// Properties and constants
			&["property_declaration", "const_declaration"],
			// Namespace and use statements
			&["namespace_definition", "use_declaration"],
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
			"function_definition" | "method_declaration" => "function declarations",
			"class_declaration" => "class declarations",
			"trait_declaration" => "trait declarations",
			"interface_declaration" => "interface declarations",
			"property_declaration" => "property declarations",
			"const_declaration" => "constant declarations",
			"namespace_definition" => "namespace declarations",
			"use_declaration" => "use statements",
			_ => "declarations",
		}
	}

	fn extract_imports_exports(&self, node: Node, contents: &str) -> (Vec<String>, Vec<String>) {
		let mut imports = Vec::new();
		let mut exports = Vec::new();

		match node.kind() {
			"namespace_use_declaration" => {
				// Handle: use Namespace\Class;
				// Handle: use Namespace\Class as Alias;
				if let Ok(use_text) = node.utf8_text(contents.as_bytes()) {
					if let Some(imported_items) = parse_php_use_statement(use_text) {
						imports.extend(imported_items);
					}
				}
			}
			"function_definition"
			| "method_declaration"
			| "class_declaration"
			| "namespace_definition" => {
				// In PHP, all top-level items are potentially exportable
				// Extract the name as a potential export
				for child in node.children(&mut node.walk()) {
					if child.kind() == "name" {
						if let Ok(name) = child.utf8_text(contents.as_bytes()) {
							exports.push(name.to_string());
							break;
						}
					}
				}
			}
			_ => {}
		}

		(imports, exports)
	}
}

// Helper function for PHP use statement parsing
fn parse_php_use_statement(use_text: &str) -> Option<Vec<String>> {
	let mut imports = Vec::new();
	let cleaned = use_text.trim();

	// Handle: use Namespace\Class;
	// Handle: use Namespace\Class as Alias;
	if let Some(rest) = cleaned.strip_prefix("use ") {
		let rest = rest.trim_end_matches(';'); // Skip trailing ";"

		// Handle: use Namespace\Class as Alias;
		if let Some(as_pos) = rest.find(" as ") {
			let class_path = &rest[..as_pos];
			if let Some(class_name) = class_path.split('\\').next_back() {
				imports.push(class_name.to_string());
			}
		} else {
			// Handle: use Namespace\Class;
			if let Some(class_name) = rest.split('\\').next_back() {
				imports.push(class_name.to_string());
			}
		}
		return Some(imports);
	}

	None
}
