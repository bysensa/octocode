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
}
