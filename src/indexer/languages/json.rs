//! JSON language implementation for the indexer

use tree_sitter::Node;
use crate::indexer::languages::Language;

pub struct Json {}

impl Language for Json {
	fn name(&self) -> &'static str {
		"json"
	}

	fn get_ts_language(&self) -> tree_sitter::Language {
		tree_sitter_json::LANGUAGE.into()
	}

	fn get_meaningful_kinds(&self) -> Vec<&'static str> {
		vec![
			"object",
			"array",
		]
	}

	fn extract_symbols(&self, node: Node, contents: &str) -> Vec<String> {
		let mut symbols = Vec::new();

		// For JSON we mostly care about property names (keys) in objects
		if node.kind() == "object" {
			self.extract_json_keys(node, contents, &mut symbols);
		} else {
			self.extract_identifiers(node, contents, &mut symbols);
		}

		// Deduplicate symbols before returning
		symbols.sort();
		symbols.dedup();

		symbols
	}

	fn extract_identifiers(&self, node: Node, contents: &str, symbols: &mut Vec<String>) {
		let kind = node.kind();
		// For JSON, we're mostly interested in property names (keys)
		if kind == "string" {
			let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
			if parent_kind == "pair" {
				// This is likely a key in a JSON object
				if let Ok(text) = node.utf8_text(contents.as_bytes()) {
					// Strip the quotes from the string
					let t = text.trim_matches('"').trim();
					if !t.is_empty() && !symbols.contains(&t.to_string()) {
						symbols.push(t.to_string());
					}
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

		// JSON-specific semantic groups
		let semantic_groups = [
			// JSON structures
			&["object", "array"] as &[&str],
			// JSON values  
			&["string", "number", "true", "false", "null"],
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
			"object" => "JSON objects",
			"array" => "JSON arrays",
			"string" => "JSON strings",
			"number" => "JSON numbers",
			"true" | "false" => "JSON booleans",
			"null" => "JSON null values",
			"pair" => "JSON key-value pairs",
			_ => "JSON structures",
		}
	}
}

impl Json {
	/// Extract key names from JSON objects
	#[allow(clippy::only_used_in_recursion)]
	fn extract_json_keys(&self, node: Node, contents: &str, symbols: &mut Vec<String>) {
		for child in node.children(&mut node.walk()) {
			if child.kind() == "pair" {
				let mut pair_cursor = child.walk();
				if pair_cursor.goto_first_child() {
					// The first child of a pair is the key
					let key_node = pair_cursor.node();
					if key_node.kind() == "string" {
						if let Ok(text) = key_node.utf8_text(contents.as_bytes()) {
							// Strip the quotes from the string
							let t = text.trim_matches('"').trim();
							if !t.is_empty() && !symbols.contains(&t.to_string()) {
								symbols.push(t.to_string());
							}
						}
					}
				}

				// Check if this is a nested object
				if pair_cursor.goto_next_sibling() && pair_cursor.goto_next_sibling() {  // Skip the colon
					let value_node = pair_cursor.node();
					if value_node.kind() == "object" || value_node.kind() == "array" {
						self.extract_json_keys(value_node, contents, symbols);
					}
				}
			}
		}
	}
}
