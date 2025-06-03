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

//! JavaScript language implementation for the indexer

use crate::indexer::languages::Language;
use tree_sitter::Node;

pub struct JavaScript {}

impl Language for JavaScript {
	fn name(&self) -> &'static str {
		"javascript"
	}

	fn get_ts_language(&self) -> tree_sitter::Language {
		tree_sitter_javascript::LANGUAGE.into()
	}

	fn get_meaningful_kinds(&self) -> Vec<&'static str> {
		vec![
			"function_declaration",
			"method_definition",
			"arrow_function",
			// Removed: "class_declaration" - too large, not semantic
			// Individual methods inside classes will be captured via method_definition
		]
	}

	fn extract_symbols(&self, node: Node, contents: &str) -> Vec<String> {
		let mut symbols = Vec::new();

		match node.kind() {
			"function_declaration" | "method_definition" => {
				// Extract name of the function or method
				for child in node.children(&mut node.walk()) {
					if child.kind() == "identifier" || child.kind().contains("name") {
						if let Ok(n) = child.utf8_text(contents.as_bytes()) {
							symbols.push(n.to_string());
						}
						break;
					}
				}

				// Look for variable declarations within the function/method body
				for child in node.children(&mut node.walk()) {
					if child.kind() == "statement_block" {
						self.extract_js_variable_declarations(child, contents, &mut symbols);
						break;
					}
				}
			}
			"arrow_function" => {
				// Extract parent variable name for arrow functions
				if let Some(parent) = node.parent() {
					if parent.kind() == "variable_declarator" {
						for child in parent.children(&mut parent.walk()) {
							if child.kind() == "identifier" {
								if let Ok(n) = child.utf8_text(contents.as_bytes()) {
									symbols.push(n.to_string());
								}
								break;
							}
						}
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
		if (kind.contains("identifier") || kind.contains("name")) && kind != "property_identifier" {
			if let Ok(text) = node.utf8_text(contents.as_bytes()) {
				let t = text.trim();
				if !t.is_empty() && !symbols.contains(&t.to_string()) {
					symbols.push(t.to_string());
				}
			}
		}

		// For JavaScript avoid excessive recursion into certain nodes
		// that tend to duplicate identifiers
		if node.kind() == "member_expression" || node.kind() == "property_access_expression" {
			// For member expressions, only take the object part (leftmost identifier)
			let mut cursor = node.walk();
			if cursor.goto_first_child() {
				// Process just the first child (object)
				self.extract_identifiers(cursor.node(), contents, symbols);
				return;
			}
		}

		// Continue with normal recursion for other nodes
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

		// JavaScript-specific semantic groups
		let semantic_groups = [
			// Functions and methods
			&[
				"function_declaration",
				"method_definition",
				"arrow_function",
			] as &[&str],
			// Classes and constructors
			&["class_declaration", "method_definition"],
			// Import/export statements
			&["import_statement", "export_statement"],
			// Variable declarations
			&["variable_declaration", "lexical_declaration"],
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
			"function_declaration" | "method_definition" | "arrow_function" => {
				"function declarations"
			}
			"class_declaration" => "class declarations",
			"import_statement" | "export_statement" => "import/export statements",
			"variable_declaration" | "lexical_declaration" => "variable declarations",
			_ => "declarations",
		}
	}
}

impl JavaScript {
	/// Extract JavaScript variable declarations within a block
	#[allow(clippy::only_used_in_recursion)]
	pub fn extract_js_variable_declarations(
		&self,
		node: Node,
		contents: &str,
		symbols: &mut Vec<String>,
	) {
		let mut cursor = node.walk();
		// Look through all children
		if cursor.goto_first_child() {
			loop {
				let child = cursor.node();
				// Look for variable or lexical declarations
				if child.kind() == "variable_declaration" || child.kind() == "lexical_declaration" {
					// For each declarator in the declaration
					for var_decl in child.children(&mut child.walk()) {
						if var_decl.kind() == "variable_declarator" {
							// Get the identifier from the declarator
							for decl_child in var_decl.children(&mut var_decl.walk()) {
								if decl_child.kind() == "identifier" {
									if let Ok(name) = decl_child.utf8_text(contents.as_bytes()) {
										let t = name.trim();
										if !t.is_empty() && !symbols.contains(&t.to_string()) {
											symbols.push(t.to_string());
										}
									}
									break; // Only take the first identifier (the variable name)
								}
							}
						}
					}
				}
				// Recursive search in nested blocks (if, for, while loops, etc.)
				else if child.kind() == "statement_block" || child.kind().contains("statement") {
					self.extract_js_variable_declarations(child, contents, symbols);
				}

				if !cursor.goto_next_sibling() {
					break;
				}
			}
		}
	}
}
