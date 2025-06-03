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

// Module for search functionality

use crate::config::Config;
use crate::store::{CodeBlock, Store};
use anyhow::Result;
use std::collections::HashSet;

// Render code blocks in a user-friendly format
pub fn render_code_blocks(blocks: &[CodeBlock]) {
	render_code_blocks_with_config(blocks, &Config::default());
}

// Render code blocks in a user-friendly format with configuration
pub fn render_code_blocks_with_config(blocks: &[CodeBlock], config: &Config) {
	if blocks.is_empty() {
		println!("No code blocks found for the query.");
		return;
	}

	println!("Found {} code blocks:\n", blocks.len());

	// Display results in their sorted order (most relevant first)
	for (idx, block) in blocks.iter().enumerate() {
		println!(
			"╔══════════════════ File: {} ══════════════════",
			block.path
		);
		println!("║");
		println!("║ Result {} of {}", idx + 1, blocks.len());
		println!("║ Language: {}", block.language);
		println!("║ Lines: {}-{}", block.start_line, block.end_line);

		// Show similarity score if available
		if let Some(distance) = block.distance {
			println!("║ Similarity: {:.4}", 1.0 - distance);
		}

		if !block.symbols.is_empty() {
			println!("║ Symbols:");
			// Deduplicate symbols in display
			let mut display_symbols = block.symbols.clone();
			display_symbols.sort();
			display_symbols.dedup();

			for symbol in display_symbols {
				// Only show non-type symbols to users
				if !symbol.contains("_") {
					println!("║   • {}", symbol);
				}
			}
		}

		println!("║ Content:");
		println!("║ ┌────────────────────────────────────");

		// Use smart truncation based on configuration
		let max_chars = config.search.search_block_max_characters;
		let (content, was_truncated) =
			crate::indexer::truncate_content_smartly(&block.content, max_chars);

		// Display content with proper indentation
		for line in content.lines() {
			println!("║ │ {}", line);
		}

		// Add note if content was truncated
		if was_truncated {
			println!("║ │ [Content truncated - limit: {} chars]", max_chars);
		}

		println!("║ └────────────────────────────────────");
		println!("╚════════════════════════════════════════\n");
	}
}

// Render search results as JSON
pub fn render_results_json(results: &[CodeBlock]) -> Result<(), anyhow::Error> {
	let json = serde_json::to_string_pretty(results)?;
	println!("{}", json);
	Ok(())
}

// Expand symbols in code blocks to include related code while maintaining relevance order
pub async fn expand_symbols(
	store: &Store,
	code_blocks: Vec<CodeBlock>,
) -> Result<Vec<CodeBlock>, anyhow::Error> {
	// We'll keep original blocks at the top with their original order
	let mut expanded_blocks = Vec::new();
	let mut original_hashes = HashSet::new();

	// Add original blocks and keep track of their hashes
	for block in &code_blocks {
		expanded_blocks.push(block.clone());
		original_hashes.insert(block.hash.clone());
	}

	let mut symbol_refs = Vec::new();

	// Collect all symbols from the code blocks
	for block in &code_blocks {
		for symbol in &block.symbols {
			// Skip the type symbols (like "function_definition") and only include actual named symbols
			if !symbol.contains("_") && symbol.chars().next().is_some_and(|c| c.is_alphabetic()) {
				symbol_refs.push(symbol.clone());
			}
		}
	}

	// Deduplicate symbols
	symbol_refs.sort();
	symbol_refs.dedup();

	println!("Found {} unique symbols to expand", symbol_refs.len());

	// Store expanded blocks to sort them by symbol count later
	let mut additional_blocks = Vec::new();

	// For each symbol, find code blocks that contain it
	for symbol in &symbol_refs {
		// Use a reference to avoid moving symbol_refs
		if let Some(block) = store.get_code_block_by_symbol(symbol).await? {
			// Check if we already have this block (avoid duplicates)
			if !original_hashes.contains(&block.hash)
				&& !additional_blocks
					.iter()
					.any(|b: &CodeBlock| b.hash == block.hash)
			{
				// Add dependencies we haven't seen before
				additional_blocks.push(block);
			}
		}
	}

	// Sort additional blocks by symbol count (more symbols = more relevant)
	// This is a heuristic to put more complex/relevant blocks first
	additional_blocks.sort_by(|a, b| {
		// First try to sort by number of matching symbols (more matches = more relevant)
		let a_matches = a.symbols.iter().filter(|s| symbol_refs.contains(s)).count();
		let b_matches = b.symbols.iter().filter(|s| symbol_refs.contains(s)).count();

		// Primary sort by symbol match count (descending)
		let match_cmp = b_matches.cmp(&a_matches);

		if match_cmp == std::cmp::Ordering::Equal {
			// Secondary sort by file path and line number when match counts are equal
			let path_cmp = a.path.cmp(&b.path);
			if path_cmp == std::cmp::Ordering::Equal {
				a.start_line.cmp(&b.start_line)
			} else {
				path_cmp
			}
		} else {
			match_cmp
		}
	});

	// Add the sorted additional blocks to our results
	expanded_blocks.extend(additional_blocks);

	Ok(expanded_blocks)
}

// Search function for MCP server - returns formatted markdown results
pub async fn search_codebase(query: &str, mode: &str, config: &Config) -> Result<String> {
	// Initialize store
	let store = Store::new().await?;

	// Generate embeddings for the query
	let embeddings = match mode {
		"code" => crate::embedding::generate_embeddings(query, true, config).await?,
		"docs" | "text" => crate::embedding::generate_embeddings(query, false, config).await?,
		_ => crate::embedding::generate_embeddings(query, true, config).await?, // Default to code model for "all"
	};

	// Perform the search based on mode
	match mode {
		"code" => {
			let results = store
				.get_code_blocks_with_config(
					embeddings,
					Some(config.search.max_results),
					Some(config.search.similarity_threshold),
				)
				.await?;
			Ok(format_code_search_results_as_markdown(&results))
		}
		"text" => {
			let results = store
				.get_text_blocks_with_config(
					embeddings,
					Some(config.search.max_results),
					Some(config.search.similarity_threshold),
				)
				.await?;
			Ok(format_text_search_results_as_markdown(&results))
		}
		"docs" => {
			let results = store
				.get_document_blocks_with_config(
					embeddings,
					Some(config.search.max_results),
					Some(config.search.similarity_threshold),
				)
				.await?;
			Ok(format_doc_search_results_as_markdown(&results))
		}
		_ => {
			// "all" mode - search across all types
			let code_results = store
				.get_code_blocks_with_config(
					embeddings.clone(),
					Some(config.search.max_results),
					Some(config.search.similarity_threshold),
				)
				.await?;
			let text_results = store
				.get_text_blocks_with_config(
					embeddings.clone(),
					Some(config.search.max_results),
					Some(config.search.similarity_threshold),
				)
				.await?;
			let doc_results = store
				.get_document_blocks_with_config(
					embeddings,
					Some(config.search.max_results),
					Some(config.search.similarity_threshold),
				)
				.await?;

			// Format combined results
			Ok(format_combined_search_results_as_markdown(
				&code_results,
				&text_results,
				&doc_results,
			))
		}
	}
}

// Format code search results as markdown for MCP
fn format_code_search_results_as_markdown(blocks: &[CodeBlock]) -> String {
	if blocks.is_empty() {
		return "No code results found.".to_string();
	}

	let mut output = String::new();
	output.push_str(&format!(
		"# Code Search Results\n\nFound {} code blocks:\n\n",
		blocks.len()
	));

	// Group blocks by file path for better organization
	let mut blocks_by_file: std::collections::HashMap<String, Vec<&CodeBlock>> =
		std::collections::HashMap::new();

	for block in blocks {
		blocks_by_file
			.entry(block.path.clone())
			.or_default()
			.push(block);
	}

	// Format results organized by file
	for (file_path, file_blocks) in blocks_by_file.iter() {
		output.push_str(&format!("## File: {}\n\n", file_path));

		for (idx, block) in file_blocks.iter().enumerate() {
			output.push_str(&format!(
				"### Block {} of {} in file\n\n",
				idx + 1,
				file_blocks.len()
			));
			output.push_str(&format!("- **Language**: {}\n", block.language));
			output.push_str(&format!(
				"- **Lines**: {}-{}\n",
				block.start_line, block.end_line
			));

			// Show similarity score if available
			if let Some(distance) = block.distance {
				output.push_str(&format!("- **Similarity**: {:.4}\n", 1.0 - distance));
			}

			if !block.symbols.is_empty() {
				output.push_str("- **Symbols**: ");
				// Deduplicate symbols in display
				let mut display_symbols = block.symbols.clone();
				display_symbols.sort();
				display_symbols.dedup();

				let relevant_symbols: Vec<String> = display_symbols
					.iter()
					.filter(|symbol| !symbol.contains("_"))
					.cloned()
					.collect();

				if !relevant_symbols.is_empty() {
					output.push_str(&relevant_symbols.join(", "));
				}
				output.push('\n');
			}

			output.push_str("\n**Content:**\n\n");
			output.push_str("```");
			output.push_str(&block.language);
			output.push('\n');
			output.push_str(&block.content);
			output.push_str("\n```\n\n");
		}
	}

	output
}

// Format text search results as markdown for MCP
fn format_text_search_results_as_markdown(blocks: &[crate::store::TextBlock]) -> String {
	if blocks.is_empty() {
		return "No text results found.".to_string();
	}

	let mut output = String::new();
	output.push_str(&format!(
		"# Text Search Results\n\nFound {} text blocks:\n\n",
		blocks.len()
	));

	// Group blocks by file path for better organization
	let mut blocks_by_file: std::collections::HashMap<String, Vec<&crate::store::TextBlock>> =
		std::collections::HashMap::new();

	for block in blocks {
		blocks_by_file
			.entry(block.path.clone())
			.or_default()
			.push(block);
	}

	// Format results organized by file
	for (file_path, file_blocks) in blocks_by_file.iter() {
		output.push_str(&format!("## File: {}\n\n", file_path));

		for (idx, block) in file_blocks.iter().enumerate() {
			output.push_str(&format!(
				"### Block {} of {} in file\n\n",
				idx + 1,
				file_blocks.len()
			));
			output.push_str(&format!("- **Language**: {}\n", block.language));
			output.push_str(&format!(
				"- **Lines**: {}-{}\n",
				block.start_line, block.end_line
			));

			// Show similarity score if available
			if let Some(distance) = block.distance {
				output.push_str(&format!("- **Similarity**: {:.4}\n", 1.0 - distance));
			}

			output.push_str("\n**Content:**\n\n");
			output.push_str("```");
			output.push_str(&block.language);
			output.push('\n');
			output.push_str(&block.content);
			output.push_str("\n```\n\n");
		}
	}

	output
}

// Format document search results as markdown for MCP
fn format_doc_search_results_as_markdown(blocks: &[crate::store::DocumentBlock]) -> String {
	if blocks.is_empty() {
		return "No documentation results found.".to_string();
	}

	let mut output = String::new();
	output.push_str(&format!(
		"# Documentation Search Results\n\nFound {} documentation sections:\n\n",
		blocks.len()
	));

	// Group blocks by file path for better organization
	let mut blocks_by_file: std::collections::HashMap<String, Vec<&crate::store::DocumentBlock>> =
		std::collections::HashMap::new();

	for block in blocks {
		blocks_by_file
			.entry(block.path.clone())
			.or_default()
			.push(block);
	}

	// Format results organized by file
	for (file_path, file_blocks) in blocks_by_file.iter() {
		output.push_str(&format!("## File: {}\n\n", file_path));

		for (idx, block) in file_blocks.iter().enumerate() {
			output.push_str(&format!(
				"### Section {} of {} in file\n\n",
				idx + 1,
				file_blocks.len()
			));
			output.push_str(&format!("- **Title**: {}\n", block.title));
			output.push_str(&format!("- **Level**: {}\n", block.level));
			output.push_str(&format!(
				"- **Lines**: {}-{}\n",
				block.start_line, block.end_line
			));

			// Show similarity score if available
			if let Some(distance) = block.distance {
				output.push_str(&format!("- **Similarity**: {:.4}\n", 1.0 - distance));
			}

			output.push_str("\n**Content:**\n\n");
			output.push_str(&block.content);
			output.push_str("\n\n");
		}
	}

	output
}

// Format combined search results as markdown for MCP
fn format_combined_search_results_as_markdown(
	code_blocks: &[CodeBlock],
	text_blocks: &[crate::store::TextBlock],
	doc_blocks: &[crate::store::DocumentBlock],
) -> String {
	let mut output = String::new();
	output.push_str("# Combined Search Results\n\n");

	let total_results = code_blocks.len() + text_blocks.len() + doc_blocks.len();
	if total_results == 0 {
		return "No results found.".to_string();
	}

	output.push_str(&format!("Found {} total results:\n\n", total_results));

	// Documentation Results
	if !doc_blocks.is_empty() {
		output.push_str(&format_doc_search_results_as_markdown(doc_blocks));
		output.push('\n');
	}

	// Code Results
	if !code_blocks.is_empty() {
		output.push_str(&format_code_search_results_as_markdown(code_blocks));
		output.push('\n');
	}

	// Text Results
	if !text_blocks.is_empty() {
		output.push_str(&format_text_search_results_as_markdown(text_blocks));
	}

	output
}
