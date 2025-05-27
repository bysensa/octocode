//! PHP language implementation for the indexer

use tree_sitter::Node;
use crate::indexer::languages::Language;

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
			// Removed: "class_declaration" - too large, not semantic
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
			},
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
				let t = if let Some(stripped) = t.strip_prefix('$') { stripped } else { t };

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
				if !cursor.goto_next_sibling() { break; }
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
			&["class_declaration", "trait_declaration", "interface_declaration"],
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
}
