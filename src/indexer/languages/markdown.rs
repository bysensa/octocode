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

//! Markdown language support for signature extraction
//! Extracts headings as signatures with smart content preview

use super::Language;
use tree_sitter::Node;

/// Markdown language implementation
pub struct Markdown;

impl Language for Markdown {
	fn name(&self) -> &'static str {
		"markdown"
	}

	fn get_ts_language(&self) -> tree_sitter::Language {
		// For markdown, we'll use a simple text-based approach since tree-sitter doesn't have
		// a reliable markdown parser. We'll create a dummy language that matches everything.
		// This is a placeholder - we'll handle markdown parsing in extract_symbols
		tree_sitter_json::LANGUAGE.into()
	}

	fn get_meaningful_kinds(&self) -> Vec<&'static str> {
		// For markdown, we'll handle this in extract_symbols instead
		// Return empty vec since we don't use tree-sitter parsing for markdown
		vec![]
	}

	fn extract_symbols(&self, _node: Node, contents: &str) -> Vec<String> {
		// Extract markdown headings as symbols
		let mut symbols = Vec::new();

		for line in contents.lines() {
			let trimmed = line.trim();
			if trimmed.starts_with('#') && !trimmed.starts_with("```") {
				// Extract heading text (remove # and trim)
				let heading_text = trimmed.trim_start_matches('#').trim();
				if !heading_text.is_empty() {
					symbols.push(heading_text.to_string());
				}
			}
		}

		symbols
	}

	fn extract_identifiers(&self, _node: Node, _contents: &str, _symbols: &mut Vec<String>) {
		// Not used for markdown
	}

	fn get_node_type_description(&self, _node_type: &str) -> &'static str {
		"markdown headings"
	}
}
