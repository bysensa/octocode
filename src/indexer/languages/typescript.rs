//! TypeScript language implementation for the indexer

use tree_sitter::Node;
use crate::indexer::languages::{Language, JavaScript};

pub struct TypeScript {}

impl Language for TypeScript {
	fn name(&self) -> &'static str {
		"typescript"
	}

	fn get_ts_language(&self) -> tree_sitter::Language {
		tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
	}

	fn get_meaningful_kinds(&self) -> Vec<&'static str> {
		vec![
			"function_declaration",
			"method_definition",
			"class_declaration",
			"arrow_function",
			"interface_declaration",
			"type_alias_declaration",
		]
	}

	fn extract_symbols(&self, node: Node, contents: &str) -> Vec<String> {
		let mut symbols = Vec::new();

		match node.kind() {
			"function_declaration" | "method_definition" | "class_declaration" |
			"interface_declaration" | "type_alias_declaration" => {
				// Extract name of the function, method, class, interface or type
				for child in node.children(&mut node.walk()) {
					if child.kind() == "identifier" || child.kind().contains("name") {
						if let Ok(n) = child.utf8_text(contents.as_bytes()) {
							symbols.push(n.to_string());
						}
						break;
					}
				}

				// For functions and methods, look for variable declarations within the body
				if node.kind() == "function_declaration" || node.kind() == "method_definition" {
					for child in node.children(&mut node.walk()) {
						if child.kind() == "statement_block" {
							let js = JavaScript {};
							js.extract_js_variable_declarations(child, contents, &mut symbols);
							break;
						}
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
			_ => {
				let js = JavaScript {};
				js.extract_identifiers(node, contents, &mut symbols);
			},
		}

		// Deduplicate symbols before returning
		symbols.sort();
		symbols.dedup();

		symbols
	}

	fn extract_identifiers(&self, node: Node, contents: &str, symbols: &mut Vec<String>) {
		// Reuse JavaScript's identifier extraction logic
		let js = JavaScript {};
		js.extract_identifiers(node, contents, symbols);
	}

	fn are_node_types_equivalent(&self, type1: &str, type2: &str) -> bool {
		// Direct match
		if type1 == type2 {
			return true;
		}

		// TypeScript-specific semantic groups
		let semantic_groups = [
			// Functions and methods
			&["function_declaration", "method_definition", "arrow_function"] as &[&str],
			// Classes and interfaces  
			&["class_declaration", "interface_declaration"],
			// Type definitions
			&["type_alias_declaration", "interface_declaration"],
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
			"function_declaration" | "method_definition" | "arrow_function" => "function declarations",
			"class_declaration" => "class declarations",
			"interface_declaration" => "interface declarations",
			"type_alias_declaration" => "type declarations",
			"import_statement" | "export_statement" => "import/export statements",
			"variable_declaration" | "lexical_declaration" => "variable declarations",
			_ => "declarations",
		}
	}
}
