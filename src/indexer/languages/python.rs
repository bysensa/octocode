//! Python language implementation for the indexer

use tree_sitter::Node;
use crate::indexer::languages::Language;

pub struct Python {}

impl Language for Python {
	fn name(&self) -> &'static str {
		"python"
	}

	fn get_ts_language(&self) -> tree_sitter::Language {
		tree_sitter_python::LANGUAGE.into()
	}

	fn get_meaningful_kinds(&self) -> Vec<&'static str> {
		vec![
			"function_definition",
			// Removed: "class_definition" - too large, not semantic
			// Individual methods inside classes will be extracted separately if needed
		]
	}

	fn extract_symbols(&self, node: Node, contents: &str) -> Vec<String> {
		let mut symbols = Vec::new();

		match node.kind() {
			"function_definition" => {
				// Find the identifier (name) node for the function
				for child in node.children(&mut node.walk()) {
					if child.kind() == "identifier" {
						if let Ok(name) = child.utf8_text(contents.as_bytes()) {
							symbols.push(name.to_string());
						}
						break;
					}
				}

				// Extract variable assignments within the function
				for child in node.children(&mut node.walk()) {
					if child.kind() == "block" {
						self.extract_python_variables(child, contents, &mut symbols);
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
		// Check if this is a valid identifier
		if kind == "identifier" {
			if let Ok(text) = node.utf8_text(contents.as_bytes()) {
				let t = text.trim();
				if !t.is_empty() && !symbols.contains(&t.to_string()) && !t.starts_with("_") {
					symbols.push(t.to_string());
				}
			}
		}

		// Continue with normal recursion for other nodes
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

		// Python-specific semantic groups
		let semantic_groups = [
			// Functions and methods
			&["function_definition"] as &[&str],
			// Classes
			&["class_definition"],
			// Import statements
			&["import_statement", "import_from_statement"],
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
			"class_definition" => "class declarations",
			"import_statement" | "import_from_statement" => "import statements",
			_ => "declarations",
		}
	}
}

impl Python {
	/// Extract variable assignments in Python blocks
	#[allow(clippy::only_used_in_recursion)]
	fn extract_python_variables(&self, node: Node, contents: &str, symbols: &mut Vec<String>) {
		let mut cursor = node.walk();
		if cursor.goto_first_child() {
			loop {
				let child = cursor.node();

				// Handle different types of assignments
				match child.kind() {
					"assignment" => {
						// For regular assignments like 'x = 10'
						let mut child_cursor = child.walk();
						if child_cursor.goto_first_child() {  // First child is the target
							let target = child_cursor.node();
							if target.kind() == "identifier" {
								if let Ok(name) = target.utf8_text(contents.as_bytes()) {
									if !name.starts_with("_") && !symbols.contains(&name.to_string()) {
										symbols.push(name.to_string());
									}
								}
							}
						}
					},
					"expression_statement" => {
						// Check for augmented assignments like 'x += 1'
						for expr_child in child.children(&mut child.walk()) {
							if expr_child.kind() == "augmented_assignment" {
								let mut aug_cursor = expr_child.walk();
								if aug_cursor.goto_first_child() {  // First child is target
									let target = aug_cursor.node();
									if target.kind() == "identifier" {
										if let Ok(name) = target.utf8_text(contents.as_bytes()) {
											if !name.starts_with("_") && !symbols.contains(&name.to_string()) {
												symbols.push(name.to_string());
											}
										}
									}
								}
							}
						}
					},
					"for_statement" | "while_statement" | "if_statement" | "try_statement" | "with_statement" => {
						// Recursive search in nested blocks
						for stmt_child in child.children(&mut child.walk()) {
							if stmt_child.kind() == "block" {
								self.extract_python_variables(stmt_child, contents, symbols);
							}
						}
					},
					_ => {}
				}

				if !cursor.goto_next_sibling() { break; }
			}
		}
	}
}
