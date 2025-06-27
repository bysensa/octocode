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

// GraphRAG core builder implementation

use crate::config::Config;
use crate::embedding::{
	calculate_unique_content_hash, create_embedding_provider_from_parts,
	types::parse_provider_model, EmbeddingProvider,
};
use crate::indexer::graphrag::ai::AIEnhancements;
use crate::indexer::graphrag::database::DatabaseOperations;
use crate::indexer::graphrag::relationships::RelationshipDiscovery;
use crate::indexer::graphrag::types::{CodeGraph, CodeNode, CodeRelationship};
use crate::indexer::graphrag::utils::{cosine_similarity, detect_project_root, to_relative_path};
use crate::state::SharedState;
use crate::store::{CodeBlock, Store};
use anyhow::{Context, Result};
use reqwest::Client;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

// Manages the creation and storage of the code graph with project-relative paths
pub struct GraphBuilder {
	config: Config,
	graph: Arc<RwLock<CodeGraph>>,
	embedding_provider: Arc<Box<dyn EmbeddingProvider>>,
	store: Store,
	project_root: PathBuf, // Project root for relative path calculations
	ai_enhancements: Option<AIEnhancements>,
}

impl GraphBuilder {
	pub async fn new(config: Config) -> Result<Self> {
		Self::new_with_quiet(config, false).await
	}

	pub async fn new_with_quiet(config: Config, quiet: bool) -> Result<Self> {
		// Detect project root (look for common indicators)
		let project_root = detect_project_root()?;

		// Initialize embedding provider from config (using text model for graph descriptions)
		// GraphRAG uses text embeddings for file descriptions and relationships, not code embeddings
		let model_string = &config.embedding.text_model;
		let (provider_type, model) = parse_provider_model(model_string);
		let embedding_provider = Arc::new(
			create_embedding_provider_from_parts(&provider_type, &model)
				.context("Failed to initialize embedding provider from config")?,
		);

		// Initialize the store for database access
		let store = Store::new().await?;

		// Load existing graph from database
		let db_ops = DatabaseOperations::new(&store);
		let graph = Arc::new(RwLock::new(db_ops.load_graph(&project_root, quiet).await?));

		// Initialize AI enhancements if enabled
		let client = Client::new();
		let ai_enhancements = if config.graphrag.use_llm {
			Some(AIEnhancements::new(config.clone(), client.clone()))
		} else {
			None
		};

		Ok(Self {
			config,
			graph,
			embedding_provider,
			store,
			project_root,
			ai_enhancements,
		})
	}

	// Legacy method for backward compatibility
	pub async fn new_with_ai_enhancements(
		config: Config,
		_use_ai_enhancements: bool,
	) -> Result<Self> {
		// Note: _use_ai_enhancements parameter is ignored, using config.graphrag.use_llm instead
		Self::new(config).await
	}

	// Check if LLM enhancements are enabled
	fn llm_enabled(&self) -> bool {
		self.config.graphrag.use_llm
	}

	// Convert absolute path to relative path from project root
	fn to_relative_path(&self, absolute_path: &str) -> Result<String> {
		to_relative_path(absolute_path, &self.project_root)
	}

	// Generate an embedding for node content
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		self.embedding_provider.generate_embedding(text).await
	}

	// Process files efficiently using existing code blocks for better performance
	pub async fn process_files_from_codeblocks(
		&self,
		code_blocks: &[CodeBlock],
		state: Option<SharedState>,
	) -> Result<()> {
		let mut new_nodes = Vec::new();
		let mut pending_embeddings = Vec::new(); // For batch embedding generation
		let mut processed_count = 0;
		let mut skipped_count = 0;
		let mut batches_processed = 0;

		// Group code blocks by file for efficient processing
		let mut files_to_blocks: HashMap<String, Vec<&CodeBlock>> = HashMap::new();
		for block in code_blocks {
			files_to_blocks
				.entry(block.path.clone())
				.or_default()
				.push(block);
		}

		// Process each file
		for (file_path, file_blocks) in files_to_blocks {
			// Convert to relative path
			let relative_path = match self.to_relative_path(&file_path) {
				Ok(path) => path,
				Err(_) => {
					eprintln!("Warning: Skipping file outside project root: {}", file_path);
					continue;
				}
			};

			// Calculate file hash based on all blocks
			let combined_content: String = file_blocks
				.iter()
				.map(|b| b.content.as_str())
				.collect::<Vec<_>>()
				.join("\n");
			let content_hash = calculate_unique_content_hash(&combined_content, &file_path);

			// Check if we already have this file with the same hash
			let graph = self.graph.read().await;
			let needs_processing = match graph.nodes.get(&relative_path) {
				Some(existing_node) if existing_node.hash == content_hash => {
					skipped_count += 1;
					false
				}
				_ => true,
			};
			drop(graph);

			if needs_processing {
				// Extract file information efficiently
				let file_name = Path::new(&file_path)
					.file_stem()
					.and_then(|s| s.to_str())
					.unwrap_or("unknown")
					.to_string();

				// Determine file kind based on path patterns
				let kind = RelationshipDiscovery::determine_file_kind(&relative_path);

				// Extract language from the first block (should be consistent)
				let language = file_blocks
					.first()
					.map(|b| b.language.clone())
					.unwrap_or_else(|| "unknown".to_string());

				// Collect all symbols from all blocks
				let mut all_symbols = HashSet::new();
				let mut all_functions = Vec::new();
				let mut total_lines = 0;

				for block in &file_blocks {
					all_symbols.extend(block.symbols.iter().cloned());
					total_lines = total_lines.max(block.end_line);

					// Extract function information from this block
					if let Ok(functions) =
						RelationshipDiscovery::extract_functions_from_block(block)
					{
						all_functions.extend(functions);
					}
				}

				let symbols: Vec<String> = all_symbols.into_iter().collect();

				// Efficiently extract imports and exports based on language and symbols
				let (imports, exports) = RelationshipDiscovery::extract_imports_exports_efficient(
					&symbols,
					&language,
					&relative_path,
				);

				// Generate description - use AI for complex files when enabled
				let description = if self.llm_enabled()
					&& self.should_use_ai_for_description(&symbols, total_lines as u32, &language)
				{
					// Collect a meaningful content sample for AI analysis
					let content_sample = self.build_content_sample_for_ai(&file_blocks);
					self.extract_ai_description(&content_sample, &file_path, &language, &symbols)
						.await
						.unwrap_or_else(|_| {
							RelationshipDiscovery::generate_simple_description(
								&file_name,
								&language,
								&symbols,
								total_lines as u32,
							)
						})
				} else {
					RelationshipDiscovery::generate_simple_description(
						&file_name,
						&language,
						&symbols,
						total_lines as u32,
					)
				};

				// Generate summary text for embedding (much lighter than full content)
				let summary_text =
					format!("{} {} symbols: {}", file_name, language, symbols.join(" "));

				// Store summary text for batch embedding generation
				pending_embeddings.push(summary_text);

				// Create the file node without embedding (will be added later)
				let node = CodeNode {
					id: relative_path.clone(),
					name: file_name,
					kind,
					path: relative_path.clone(),
					description,
					symbols,
					imports,
					exports,
					functions: all_functions,
					hash: content_hash,
					embedding: Vec::new(), // Will be filled after batch embedding
					size_lines: total_lines as u32,
					language,
				};

				new_nodes.push(node);
				processed_count += 1;

				// Update state if provided
				if let Some(ref state) = state {
					let mut state_guard = state.write();
					state_guard.status_message = format!("Processing file: {}", file_path);
				}

				// Check if we should process batch (same logic as normal indexing)
				if self.should_process_batch(&pending_embeddings) {
					self.process_nodes_batch(
						&mut new_nodes,
						&mut pending_embeddings,
						&mut batches_processed,
					)
					.await?;
				}
			}
		}

		// Process any remaining nodes in the final batch
		if !new_nodes.is_empty() {
			self.process_nodes_batch(
				&mut new_nodes,
				&mut pending_embeddings,
				&mut batches_processed,
			)
			.await?;
		}

		// Collect all processed nodes for relationship discovery
		let all_processed_nodes = {
			let graph = self.graph.read().await;
			graph.nodes.values().cloned().collect::<Vec<CodeNode>>()
		};

		// Discover relationships efficiently for all processed nodes
		if !all_processed_nodes.is_empty() {
			// Get only the newly processed nodes for relationship discovery
			let _new_node_ids: std::collections::HashSet<String> =
				all_processed_nodes.iter().map(|n| n.id.clone()).collect();

			let relationships = if self.llm_enabled() {
				// Enhanced relationship discovery with optional AI for complex cases
				self.discover_relationships_with_ai_enhancement(&all_processed_nodes)
					.await?
			} else {
				// Fast rule-based relationship discovery only
				self.discover_relationships_efficiently(&all_processed_nodes)
					.await?
			};

			if !relationships.is_empty() {
				let mut graph = self.graph.write().await;
				graph.relationships.extend(relationships.clone());
				drop(graph);

				// Save just the relationships (nodes were already saved in batches)
				let db_ops = DatabaseOperations::new(&self.store);
				db_ops.save_graph_incremental(&[], &relationships).await?;
			}
		}

		// Final flush to ensure all data is persisted
		self.store.flush().await?;

		// Update final state
		if let Some(state) = state {
			let mut state_guard = state.write();
			state_guard.status_message = format!(
				"GraphRAG processing complete: {} files processed ({} skipped)",
				processed_count, skipped_count
			);
		} else {
			println!(
				"GraphRAG: Processed {} files ({} skipped)",
				processed_count, skipped_count
			);
		}

		Ok(())
	}

	// Enhanced relationship discovery with optional AI for complex cases
	async fn discover_relationships_with_ai_enhancement(
		&self,
		new_files: &[CodeNode],
	) -> Result<Vec<CodeRelationship>> {
		if let Some(ref ai) = self.ai_enhancements {
			// Get all nodes for context
			let all_nodes = {
				let graph = self.graph.read().await;
				graph.nodes.values().cloned().collect::<Vec<CodeNode>>()
			};
			ai.discover_relationships_with_ai_enhancement(new_files, &all_nodes)
				.await
		} else {
			// Fallback to efficient discovery without AI
			self.discover_relationships_efficiently(new_files).await
		}
	}

	// Discover relationships efficiently without AI for most cases
	async fn discover_relationships_efficiently(
		&self,
		new_files: &[CodeNode],
	) -> Result<Vec<CodeRelationship>> {
		// Get all nodes from the graph for relationship discovery
		let all_nodes = {
			let graph = self.graph.read().await;
			graph.nodes.values().cloned().collect::<Vec<CodeNode>>()
		};

		RelationshipDiscovery::discover_relationships_efficiently(new_files, &all_nodes).await
	}

	// Determine if a file is complex enough to benefit from AI analysis
	fn should_use_ai_for_description(
		&self,
		symbols: &[String],
		lines: u32,
		language: &str,
	) -> bool {
		if let Some(ref ai) = self.ai_enhancements {
			ai.should_use_ai_for_description(symbols, lines, language)
		} else {
			false
		}
	}

	// Build a meaningful content sample for AI analysis (not full file content)
	fn build_content_sample_for_ai(&self, file_blocks: &[&CodeBlock]) -> String {
		if let Some(ref ai) = self.ai_enhancements {
			ai.build_content_sample_for_ai(file_blocks)
		} else {
			String::new()
		}
	}

	// Extract AI-powered description for complex files
	async fn extract_ai_description(
		&self,
		content_sample: &str,
		file_path: &str,
		language: &str,
		symbols: &[String],
	) -> Result<String> {
		if let Some(ref ai) = self.ai_enhancements {
			ai.extract_ai_description(content_sample, file_path, language, symbols)
				.await
		} else {
			Err(anyhow::anyhow!("AI enhancements not available"))
		}
	}

	// Legacy method for backward compatibility - now uses efficient code block processing
	pub async fn process_code_blocks(
		&self,
		code_blocks: &[CodeBlock],
		state: Option<SharedState>,
	) -> Result<()> {
		// Use the new efficient method that processes code blocks directly
		self.process_files_from_codeblocks(code_blocks, state).await
	}

	// Build GraphRAG from existing database when enabled after indexing
	// This solves the critical issue where GraphRAG is enabled after database is already indexed
	pub async fn build_from_existing_database(&self, state: Option<SharedState>) -> Result<()> {
		// Update state to show we're building GraphRAG from existing data
		if let Some(ref state) = state {
			let mut state_guard = state.write();
			state_guard.status_message = "Building GraphRAG from existing database...".to_string();
		}

		// Get all existing code blocks from the database
		let all_code_blocks = self.store.get_all_code_blocks_for_graphrag().await?;

		if all_code_blocks.is_empty() {
			if let Some(ref state) = state {
				let mut state_guard = state.write();
				state_guard.status_message =
					"No code blocks found in database for GraphRAG".to_string();
			}
			return Ok(());
		}

		// Update state with the number of blocks to process
		if let Some(ref state) = state {
			let mut state_guard = state.write();
			state_guard.status_message = format!(
				"Processing {} code blocks for GraphRAG...",
				all_code_blocks.len()
			);
		}

		// Process the code blocks to build the graph
		self.process_files_from_codeblocks(&all_code_blocks, state.clone())
			.await?;

		// Final flush to ensure all data is persisted
		self.store.flush().await?;

		// Update final state
		if let Some(ref state) = state {
			let mut state_guard = state.write();
			state_guard.status_message = format!(
				"GraphRAG built from existing database: {} blocks processed",
				all_code_blocks.len()
			);
		} else {
			println!(
				"GraphRAG: Built from existing database with {} code blocks",
				all_code_blocks.len()
			);
		}

		Ok(())
	}

	// Get the full graph
	pub async fn get_graph(&self) -> Result<CodeGraph> {
		let graph = self.graph.read().await;
		Ok(graph.clone())
	}

	// Search the graph for nodes matching a query
	pub async fn search_nodes(&self, query: &str) -> Result<Vec<CodeNode>> {
		// First check if we have any nodes in memory
		let in_memory_nodes = {
			let graph = self.graph.read().await;
			!graph.nodes.is_empty()
		};

		if in_memory_nodes {
			// Use in-memory search if nodes are loaded
			return self.search_nodes_in_memory(query).await;
		} else {
			// Use database search if nodes are only in database
			return self.search_nodes_in_database(query).await;
		}
	}

	// Search for nodes in memory
	async fn search_nodes_in_memory(&self, query: &str) -> Result<Vec<CodeNode>> {
		// Generate an embedding for the query
		let query_embedding = self.generate_embedding(query).await?;

		// Find similar nodes
		let graph = self.graph.read().await;
		let nodes_array = graph.nodes.values().cloned().collect::<Vec<CodeNode>>();
		drop(graph);

		// Calculate similarity to each node
		let mut similarities: Vec<(f32, CodeNode)> = Vec::new();
		let query_lower = query.to_lowercase();

		for node in nodes_array {
			// Calculate semantic similarity
			let similarity = cosine_similarity(&query_embedding, &node.embedding);

			// Check if the query is a substring of various node fields
			// This handles specific cases like searching for "impl"
			let name_contains = node.name.to_lowercase().contains(&query_lower);
			let kind_contains = node.kind.to_lowercase().contains(&query_lower);
			let desc_contains = node.description.to_lowercase().contains(&query_lower);
			let symbols_contain = node
				.symbols
				.iter()
				.any(|s| s.to_lowercase().contains(&query_lower));

			// Use a lower threshold for semantic similarity (0.5 instead of 0.6)
			// OR include if the query is a substring of any important field
			if similarity > 0.5
				|| name_contains
				|| kind_contains
				|| desc_contains
				|| symbols_contain
			{
				// Boost similarity score for exact matches to ensure they appear at the top
				let boosted_similarity = if name_contains || kind_contains || symbols_contain {
					// Ensure exact matches get higher priority
					0.9_f32.max(similarity)
				} else {
					similarity
				};

				similarities.push((boosted_similarity, node));
			}
		}

		// Sort by similarity (highest first)
		similarities.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

		// Return the nodes (without the similarity scores)
		let results = similarities.into_iter().map(|(_, node)| node).collect();

		Ok(results)
	}

	// Search for nodes in database
	async fn search_nodes_in_database(&self, query: &str) -> Result<Vec<CodeNode>> {
		// Generate an embedding for the query
		let query_embedding = self.generate_embedding(query).await?;

		let db_ops = DatabaseOperations::new(&self.store);
		db_ops
			.search_nodes_in_database(&query_embedding, query)
			.await
	}

	// Find paths between nodes in the graph
	pub async fn find_paths(
		&self,
		source_id: &str,
		target_id: &str,
		max_depth: usize,
	) -> Result<Vec<Vec<String>>> {
		let graph = self.graph.read().await;

		// Ensure both nodes exist
		if !graph.nodes.contains_key(source_id) || !graph.nodes.contains_key(target_id) {
			return Ok(Vec::new());
		}

		// Build an adjacency list for easier traversal
		let mut adjacency_list: HashMap<String, Vec<String>> = HashMap::new();
		for rel in &graph.relationships {
			adjacency_list
				.entry(rel.source.clone())
				.or_default()
				.push(rel.target.clone());
		}

		// Use BFS to find paths
		let mut queue = Vec::new();
		queue.push(vec![source_id.to_string()]);

		let mut paths = Vec::new();

		while let Some(path) = queue.pop() {
			let current = path.last().unwrap();

			// Found a path to target
			if current == target_id {
				paths.push(path);
				continue;
			}

			// Stop if we've reached max depth
			if path.len() > max_depth {
				continue;
			}

			// Explore neighbors
			if let Some(neighbors) = adjacency_list.get(current) {
				for neighbor in neighbors {
					// Avoid cycles
					if !path.contains(neighbor) {
						let mut new_path = path.clone();
						new_path.push(neighbor.clone());
						queue.push(new_path);
					}
				}
			}
		}

		Ok(paths)
	}

	// Check if we should process batch (same logic as normal indexing)
	fn should_process_batch(&self, pending_embeddings: &[String]) -> bool {
		// Use the same batch size logic as normal indexing
		let batch_size = self.config.index.embeddings_batch_size;
		let max_tokens = self.config.index.embeddings_max_tokens_per_batch;

		if pending_embeddings.len() >= batch_size {
			return true;
		}

		// Check token count (approximate)
		let total_tokens: usize = pending_embeddings.iter().map(|s| s.len() / 4).sum(); // Rough token estimate
		total_tokens >= max_tokens
	}

	// Process a batch of nodes with embeddings and persist them
	async fn process_nodes_batch(
		&self,
		nodes: &mut Vec<CodeNode>,
		pending_embeddings: &mut Vec<String>,
		batches_processed: &mut usize,
	) -> Result<()> {
		if nodes.is_empty() || pending_embeddings.is_empty() {
			return Ok(());
		}

		// Generate embeddings in batch (same as normal indexing)
		let embeddings = crate::embedding::generate_embeddings_batch(
			pending_embeddings.clone(),
			false, // Use text embeddings for GraphRAG descriptions
			&self.config,
			crate::embedding::types::InputType::Document,
		)
		.await?;

		// Assign embeddings to nodes
		for (node, embedding) in nodes.iter_mut().zip(embeddings.iter()) {
			node.embedding = embedding.clone();
		}

		// Add nodes to the graph
		{
			let mut graph = self.graph.write().await;
			for node in nodes.iter() {
				graph.nodes.insert(node.id.clone(), node.clone());
			}
		}

		// Persist nodes to database (same as normal indexing)
		let db_ops = DatabaseOperations::new(&self.store);
		db_ops.save_graph_incremental(nodes, &[]).await?;

		// Clear the batches
		nodes.clear();
		pending_embeddings.clear();
		*batches_processed += 1;

		// Use the same flush logic as normal indexing
		self.flush_if_needed(batches_processed).await?;

		Ok(())
	}

	// Flush if needed (same logic as normal indexing)
	async fn flush_if_needed(&self, batches_processed: &mut usize) -> Result<()> {
		if *batches_processed >= self.config.index.flush_frequency {
			self.store.flush().await?;
			*batches_processed = 0;
		}
		Ok(())
	}
}
