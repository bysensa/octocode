//! Bash language implementation for the indexer

use tree_sitter::Node;
use crate::indexer::languages::Language;

pub struct Bash {}

impl Language for Bash {
	fn name(&self) -> &'static str {
		"bash"
	}

	fn get_ts_language(&self) -> tree_sitter::Language {
		tree_sitter_bash::LANGUAGE.into()
	}

	fn get_meaningful_kinds(&self) -> Vec<&'static str> {
		vec![
			"function_definition",
		]
	}

	fn extract_symbols(&self, node: Node, contents: &str) -> Vec<String> {
		let mut symbols = Vec::new();

		match node.kind() {
			"function_definition" => {
				// Find the function name
				for child in node.children(&mut node.walk()) {
					if child.kind() == "name" {
						if let Ok(name) = child.utf8_text(contents.as_bytes()) {
							symbols.push(name.to_string());
						}
						break;
					}
				}

				// Extract variables from function body
				self.extract_bash_variables(node, contents, &mut symbols);
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
		// Check if this is a variable name or command name
		if kind == "variable_name" || kind == "command_name" {
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
				if !cursor.goto_next_sibling() { break; }
			}
		}
	}

	fn are_node_types_equivalent(&self, type1: &str, type2: &str) -> bool {
		// Direct match
		if type1 == type2 {
			return true;
		}

		// Bash-specific semantic groups
		let semantic_groups = [
			// Functions
			&["function_definition"] as &[&str],
			// Variable assignments and declarations
			&["variable_assignment"],
			// Commands and aliases
			&["command", "simple_command"],
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
			"function_definition" => "function declarations",
			"variable_assignment" => "variable assignments",
			"command" | "simple_command" => "command declarations",
			_ => "declarations",
		}
	}
}

impl Bash {
	/// Extract variable assignments in bash
	fn extract_bash_variables(&self, node: Node, contents: &str, symbols: &mut Vec<String>) {
		let mut cursor = node.walk();
		if cursor.goto_first_child() {
			loop {
				let child = cursor.node();

				if child.kind() == "variable_assignment" {
					for var_child in child.children(&mut child.walk()) {
						if var_child.kind() == "variable_name" {
							if let Ok(name) = var_child.utf8_text(contents.as_bytes()) {
								if !symbols.contains(&name.to_string()) {
									symbols.push(name.to_string());
								}
							}
							break;
						}
					}
				}

				// Continue traversing for nested assignments
				self.extract_identifiers(child, contents, symbols);

				if !cursor.goto_next_sibling() { break; }
			}
		}
	}
}
