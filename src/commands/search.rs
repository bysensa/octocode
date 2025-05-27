use clap::Args;

use octocode::config::Config;
use octocode::store::Store;
use octocode::indexer;
use octocode::reranker::Reranker;
use octocode::embedding;

#[derive(Args, Debug)]
pub struct SearchArgs {
	/// Search query
	pub query: String,

	/// Expand all symbols in matching code blocks
	#[arg(long, short)]
	pub expand: bool,

	/// Output in JSON format
	#[arg(long)]
	pub json: bool,

	/// Output in Markdown format
	#[arg(long)]
	pub md: bool,

	/// Search mode: all (default), code, docs, or text
	#[arg(long, default_value = "all")]
	pub mode: String,

	/// Similarity threshold (0.0-1.0). Higher values = more strict matching. Lower values = more results.
	/// Examples: 0.3 (broad results), 0.5 (balanced), 0.7 (high quality), 0.8 (very strict)
	#[arg(long, short = 't', default_value = "0.5")]
	pub threshold: f32,
}

pub async fn execute(store: &Store, args: &SearchArgs, config: &Config) -> Result<(), anyhow::Error> {
	let current_dir = std::env::current_dir()?;
	let octocode_dir = current_dir.join(".octocode");
	let index_path = octocode_dir.join("storage");

	// Check if we have an index already; if not, inform the user but don't auto-index
	if !index_path.exists() {
		return Err(anyhow::anyhow!("No index found. Please run 'octocode index' first to create an index."));
	}

	// Validate similarity threshold
	if args.threshold < 0.0 || args.threshold > 1.0 {
		return Err(anyhow::anyhow!("Similarity threshold must be between 0.0 and 1.0, got: {}", args.threshold));
	}

	// Validate search mode
	let search_mode = match args.mode.as_str() {
		"all" | "code" | "docs" | "text" => args.mode.as_str(),
		_ => {
			return Err(anyhow::anyhow!("Invalid search mode '{}'. Use 'all', 'code', 'docs', or 'text'.", args.mode));
		}
	};

	// Generate embeddings for the query based on search mode
	let (code_embeddings, text_embeddings) = match search_mode {
		"code" => {
			// Use code model for code searches
			let embeddings = embedding::generate_embeddings(&args.query, true, config).await?;
			(embeddings, vec![]) // Only need code embeddings
		},
		"docs" | "text" => {
			// Use text model for documents and text searches
			let embeddings = embedding::generate_embeddings(&args.query, false, config).await?;
			(vec![], embeddings) // Only need text embeddings
		},
		"all" => {
			// For "all" mode, we need to check if code and text models are different
			// If different, generate separate embeddings; if same, use one set
			let code_model = config.embedding.get_model(&config.embedding.provider, true);
			let text_model = config.embedding.get_model(&config.embedding.provider, false);
			
			if code_model == text_model {
				// Same model for both - generate once and reuse
				let embeddings = embedding::generate_embeddings(&args.query, true, config).await?;
				(embeddings.clone(), embeddings)
			} else {
				// Different models - generate separate embeddings
				let code_embeddings = embedding::generate_embeddings(&args.query, true, config).await?;
				let text_embeddings = embedding::generate_embeddings(&args.query, false, config).await?;
				(code_embeddings, text_embeddings)
			}
		},
		_ => unreachable!(),
	};

	// Convert similarity threshold to distance threshold once
	// Distance = 1.0 - Similarity (for cosine distance)
	// Use command-line parameter instead of config
	let distance_threshold = 1.0 - args.threshold;

	// Search based on mode
	match search_mode {
		"code" => {
			// Search only code blocks with higher limit for reranking
			let mut results = store.get_code_blocks_with_config(
				code_embeddings,
				Some(config.search.max_results * 2), // Get more results for reranking
				Some(1.01) // Use a very permissive threshold initially
			).await?;

			// Apply reranking to improve relevance
			results = Reranker::rerank_code_blocks(results, &args.query);

			// Apply tf-idf boost for better term matching
			Reranker::tf_idf_boost(&mut results, &args.query);

			// Apply final similarity threshold after reranking
			results.retain(|block| {
				if let Some(distance) = block.distance {
					distance <= distance_threshold
				} else {
					true
				}
			});

			// Limit to requested max_results
			results.truncate(config.search.max_results);

			// If expand flag is set, expand symbols in the results
			if args.expand {
				println!("Expanding symbols...");
				results = indexer::expand_symbols(store, results).await?;
			}

			// Output the results
			if args.json {
				indexer::render_results_json(&results)?
			} else if args.md {
				let markdown = indexer::code_blocks_to_markdown_with_config(&results, config);
				println!("{}", markdown);
			} else {
				indexer::render_code_blocks_with_config(&results, config);
			}
		},
		"docs" => {
			// Search only document blocks with reranking
			let mut results = store.get_document_blocks_with_config(
				text_embeddings,
				Some(config.search.max_results * 2), // Get more results for reranking
				Some(1.01) // Use a more permissive threshold initially
			).await?;

			// Apply reranking to improve relevance
			results = Reranker::rerank_document_blocks(results, &args.query);

			// Apply final similarity threshold after reranking
			results.retain(|block| {
				if let Some(distance) = block.distance {
					distance <= distance_threshold
				} else {
					true
				}
			});

			// Limit to requested max_results
			results.truncate(config.search.max_results);

			// Output the results
			if args.json {
				let json = serde_json::to_string_pretty(&results)?;
				println!("{}", json);
			} else if args.md {
				let markdown = indexer::document_blocks_to_markdown_with_config(&results, config);
				println!("{}", markdown);
			} else {
				// Render documents in a readable format
				render_document_blocks_with_config(&results, config);
			}
		},
		"text" => {
			// Search only text blocks with reranking
			let mut results = store.get_text_blocks_with_config(
				text_embeddings,
				Some(config.search.max_results * 2), // Get more results for reranking
				Some(1.01) // Use a more permissive threshold initially
			).await?;

			// Apply reranking to improve relevance
			results = Reranker::rerank_text_blocks(results, &args.query);

			// Apply final similarity threshold after reranking
			results.retain(|block| {
				if let Some(distance) = block.distance {
					distance <= distance_threshold
				} else {
					true
				}
			});

			// Limit to requested max_results
			results.truncate(config.search.max_results);

			// Output the results
			if args.json {
				let json = serde_json::to_string_pretty(&results)?;
				println!("{}", json);
			} else if args.md {
				let markdown = indexer::text_blocks_to_markdown_with_config(&results, config);
				println!("{}", markdown);
			} else {
				// Render text blocks in a readable format
				render_text_blocks_with_config(&results, config);
			}
		},
		"all" => {
			// Search code, documents, and text blocks with reranking
			let mut code_results = store.get_code_blocks_with_config(
				code_embeddings,
				Some(config.search.max_results * 2), // Get more results for reranking
				Some(1.01) // Use a more permissive threshold initially
			).await?;
			let mut doc_results = store.get_document_blocks_with_config(
				text_embeddings.clone(),
				Some(config.search.max_results * 2), // Get more results for reranking
				Some(1.01) // Use a more permissive threshold initially
			).await?;
			let mut text_results = store.get_text_blocks_with_config(
				text_embeddings,
				Some(config.search.max_results * 2), // Get more results for reranking
				Some(1.01) // Use a more permissive threshold initially
			).await?;

			// Apply reranking to improve relevance
			code_results = Reranker::rerank_code_blocks(code_results, &args.query);
			doc_results = Reranker::rerank_document_blocks(doc_results, &args.query);
			text_results = Reranker::rerank_text_blocks(text_results, &args.query);

			// Apply tf-idf boost for code results
			Reranker::tf_idf_boost(&mut code_results, &args.query);

			// Apply final similarity threshold after reranking
			code_results.retain(|block| {
				if let Some(distance) = block.distance {
					distance <= distance_threshold
				} else {
					true
				}
			});
			doc_results.retain(|block| {
				if let Some(distance) = block.distance {
					distance <= distance_threshold
				} else {
					true
				}
			});
			text_results.retain(|block| {
				if let Some(distance) = block.distance {
					distance <= distance_threshold
				} else {
					true
				}
			});

			// If expand flag is set, expand symbols in code results
			let mut final_code_results = code_results;
			if args.expand {
				println!("Expanding symbols...");
				final_code_results = indexer::expand_symbols(store, final_code_results).await?;
			}

			// Combine and sort all results by relevance for better display order
			let mut all_results_with_scores: Vec<(f32, String, String)> = Vec::new();

			// Add code results
			for block in &final_code_results {
				if let Some(distance) = block.distance {
					all_results_with_scores.push((distance, "code".to_string(), block.path.clone()));
				}
			}

			// Add document results
			for block in &doc_results {
				if let Some(distance) = block.distance {
					all_results_with_scores.push((distance, "docs".to_string(), block.path.clone()));
				}
			}

			// Add text results
			for block in &text_results {
				if let Some(distance) = block.distance {
					all_results_with_scores.push((distance, "text".to_string(), block.path.clone()));
				}
			}

			// Sort by relevance (distance) - lower distance means higher similarity
			all_results_with_scores.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

			// Output combined results
			if args.json {
				// Create a combined JSON structure
				let combined = serde_json::json!({
					"code_blocks": final_code_results,
					"document_blocks": doc_results,
					"text_blocks": text_results
				});
				println!("{}", serde_json::to_string_pretty(&combined)?);
			} else if args.md {
				// Render all sections in markdown
				let mut combined_markdown = String::new();

				if !doc_results.is_empty() {
					combined_markdown.push_str("# Documentation Results\n\n");
					combined_markdown.push_str(&indexer::document_blocks_to_markdown_with_config(&doc_results, config));
					combined_markdown.push('\n');
				}

				if !final_code_results.is_empty() {
					combined_markdown.push_str("# Code Results\n\n");
					combined_markdown.push_str(&indexer::code_blocks_to_markdown_with_config(&final_code_results, config));
					combined_markdown.push('\n');
				}

				if !text_results.is_empty() {
					combined_markdown.push_str("# Text Results\n\n");
					combined_markdown.push_str(&indexer::text_blocks_to_markdown_with_config(&text_results, config));
				}

				if combined_markdown.is_empty() {
					combined_markdown.push_str("No results found for the query.");
				}

				println!("{}", combined_markdown);
			} else {
				// Render all sections in text format
				if !doc_results.is_empty() {
					println!("=== DOCUMENTATION RESULTS ===\n");
					render_document_blocks_with_config(&doc_results, config);
					println!("\n");
				}

				if !final_code_results.is_empty() {
					println!("=== CODE RESULTS ===\n");
					indexer::render_code_blocks_with_config(&final_code_results, config);
					println!("\n");
				}

				if !text_results.is_empty() {
					println!("=== TEXT RESULTS ===\n");
					render_text_blocks_with_config(&text_results, config);
				}

				if doc_results.is_empty() && final_code_results.is_empty() && text_results.is_empty() {
					println!("No results found for the query.");
				}
			}
		},
		_ => unreachable!(),
	}

	Ok(())
}

fn render_text_blocks_with_config(blocks: &[octocode::store::TextBlock], config: &Config) {
	if blocks.is_empty() {
		println!("No text blocks found.");
		return;
	}

	println!("Found {} text blocks:\n", blocks.len());

	// Group blocks by file path for better organization
	let mut blocks_by_file: std::collections::HashMap<String, Vec<&octocode::store::TextBlock>> = std::collections::HashMap::new();

	for block in blocks {
		blocks_by_file
			.entry(block.path.clone())
			.or_default()
			.push(block);
	}

	// Print results organized by file
	for (file_path, file_blocks) in blocks_by_file.iter() {
		println!("╔══════════════════ File: {} ══════════════════", file_path);

		for (idx, block) in file_blocks.iter().enumerate() {
			println!("║");
			println!("║ Block {} of {}: {}", idx + 1, file_blocks.len(), block.language);
			println!("║ Lines: {}-{}", block.start_line, block.end_line);

			// Show similarity score if available
			if let Some(distance) = block.distance {
				println!("║ Similarity: {:.4}", 1.0 - distance);
			}

			println!("║");
			println!("║ Content:");

			// Use smart truncation based on configuration
			let max_chars = config.search.search_block_max_characters;
			let (content, was_truncated) = indexer::truncate_content_smartly(&block.content, max_chars);

			// Display content with proper indentation
			for line in content.lines() {
				println!("║   {}", line);
			}

			// Add note if content was truncated
			if was_truncated {
				println!("║   [Content truncated - limit: {} chars]", max_chars);
			}
		}

		println!("╚════════════════════════════════════════\n");
	}
}

fn render_document_blocks_with_config(blocks: &[octocode::store::DocumentBlock], config: &Config) {
	if blocks.is_empty() {
		println!("No documentation blocks found.");
		return;
	}

	println!("Found {} documentation sections:\n", blocks.len());

	// Group blocks by file path for better organization
	let mut blocks_by_file: std::collections::HashMap<String, Vec<&octocode::store::DocumentBlock>> = std::collections::HashMap::new();

	for block in blocks {
		blocks_by_file
			.entry(block.path.clone())
			.or_default()
			.push(block);
	}

	// Print results organized by file
	for (file_path, file_blocks) in blocks_by_file.iter() {
		println!("╔══════════════════ File: {} ══════════════════", file_path);

		for (idx, block) in file_blocks.iter().enumerate() {
			println!("║");
			println!("║ Section {} of {}: {}", idx + 1, file_blocks.len(), block.title);
			println!("║ Level: {}  Lines: {}-{}", block.level, block.start_line, block.end_line);

			// Show similarity score if available
			if let Some(distance) = block.distance {
				println!("║ Similarity: {:.4}", 1.0 - distance);
			}

			println!("║");
			println!("║ Content:");

			// Use smart truncation based on configuration
			let max_chars = config.search.search_block_max_characters;
			let (content, was_truncated) = indexer::truncate_content_smartly(&block.content, max_chars);

			// Display content with proper indentation
			for line in content.lines() {
				println!("║   {}", line);
			}

			// Add note if content was truncated
			if was_truncated {
				println!("║   [Content truncated - limit: {} chars]", max_chars);
			}
		}

		println!("╚════════════════════════════════════════\n");
	}
}
