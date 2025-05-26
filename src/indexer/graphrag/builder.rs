// GraphRAG core builder implementation

use crate::config::Config;
use crate::indexer::embed::calculate_unique_content_hash;
use crate::indexer::graphrag::ai::AIEnhancements;
use crate::indexer::graphrag::database::DatabaseOperations;
use crate::indexer::graphrag::relationships::RelationshipDiscovery;
use crate::indexer::graphrag::types::{CodeGraph, CodeNode, CodeRelationship};
use crate::indexer::graphrag::utils::{cosine_similarity, detect_project_root, to_relative_path};
use crate::state::SharedState;
use crate::store::{CodeBlock, Store};
use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use reqwest::Client;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

// Manages the creation and storage of the code graph with project-relative paths
pub struct GraphBuilder {
	config: Config,
	graph: Arc<RwLock<CodeGraph>>,
	embedding_model: Arc<TextEmbedding>,
	store: Store,
	project_root: PathBuf,  // Project root for relative path calculations
	ai_enhancements: Option<AIEnhancements>,
}

impl GraphBuilder {
	pub async fn new(config: Config) -> Result<Self> {
		// Detect project root (look for common indicators)
		let project_root = detect_project_root()?;
		
		// Initialize embedding model
		let cache_dir = project_root.join(".octocode/fastembed");
		std::fs::create_dir_all(&cache_dir).context("Failed to create FastEmbed cache directory")?;

		let model = TextEmbedding::try_new(
			InitOptions::new(EmbeddingModel::AllMiniLML6V2)
				.with_show_download_progress(true)
				.with_cache_dir(cache_dir),
		).context("Failed to initialize embedding model")?;

		// Initialize the store for database access
		let store = Store::new().await?;

		// Load existing graph from database
		let db_ops = DatabaseOperations::new(&store);
		let graph = Arc::new(RwLock::new(db_ops.load_graph(&project_root).await?));

		// Initialize AI enhancements if enabled
		let client = Client::new();
		let ai_enhancements = if config.index.llm_enabled {
			Some(AIEnhancements::new(config.clone(), client.clone()))
		} else {
			None
		};

		Ok(Self {
			config,
			graph,
			embedding_model: Arc::new(model),
			store,
			project_root,
			ai_enhancements,
		})
	}

	// Legacy method for backward compatibility
	pub async fn new_with_ai_enhancements(config: Config, _use_ai_enhancements: bool) -> Result<Self> {
		// Note: _use_ai_enhancements parameter is ignored, using config.index.llm_enabled instead
		Self::new(config).await
	}

	// Check if LLM enhancements are enabled
	fn llm_enabled(&self) -> bool {
		self.config.index.llm_enabled
	}

	// Convert absolute path to relative path from project root
	fn to_relative_path(&self, absolute_path: &str) -> Result<String> {
		to_relative_path(absolute_path, &self.project_root)
	}

	// Generate an embedding for node content
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		let embeddings = self.embedding_model.embed(vec![text], None)?;
		if embeddings.is_empty() {
			return Err(anyhow::anyhow!("Failed to generate embedding"));
		}
		Ok(embeddings[0].clone())
	}

	// Process files efficiently using existing code blocks for better performance
	pub async fn process_files_from_codeblocks(&self, code_blocks: &[CodeBlock], state: Option<SharedState>) -> Result<()> {
		let mut new_nodes = Vec::new();
		let mut processed_count = 0;
		let mut skipped_count = 0;

		// Group code blocks by file for efficient processing
		let mut files_to_blocks: HashMap<String, Vec<&CodeBlock>> = HashMap::new();
		for block in code_blocks {
			files_to_blocks.entry(block.path.clone()).or_default().push(block);
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
			let combined_content: String = file_blocks.iter()
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
				},
				_ => true
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
				let language = file_blocks.first()
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
					if let Ok(functions) = RelationshipDiscovery::extract_functions_from_block(block) {
						all_functions.extend(functions);
					}
				}

				let symbols: Vec<String> = all_symbols.into_iter().collect();

				// Efficiently extract imports and exports based on language and symbols
				let (imports, exports) = RelationshipDiscovery::extract_imports_exports_efficient(&symbols, &language, &relative_path);

				// Generate description - use AI for complex files when enabled
				let description = if self.llm_enabled() && self.should_use_ai_for_description(&symbols, total_lines as u32, &language) {
					// Collect a meaningful content sample for AI analysis
					let content_sample = self.build_content_sample_for_ai(&file_blocks);
					self.extract_ai_description(&content_sample, &file_path, &language, &symbols).await
						.unwrap_or_else(|_| RelationshipDiscovery::generate_simple_description(&file_name, &language, &symbols, total_lines as u32))
				} else {
					RelationshipDiscovery::generate_simple_description(&file_name, &language, &symbols, total_lines as u32)
				};

				// Generate embedding for the file summary (much lighter than full content)
				let summary_text = format!("{} {} symbols: {}", 
					file_name, language, symbols.join(" "));
				let embedding = self.generate_embedding(&summary_text).await?;

				// Create the file node
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
					embedding,
					size_lines: total_lines as u32,
					language,
				};

				// Add the node to the graph
				let mut graph = self.graph.write().await;
				graph.nodes.insert(relative_path, node.clone());
				drop(graph);

				new_nodes.push(node);
				processed_count += 1;

				// Update state if provided
				if let Some(ref state) = state {
					let mut state_guard = state.write();
					state_guard.status_message = format!("Processing file: {}", file_path);
				}
			}
		}

		// Discover relationships efficiently (no AI needed for most relationships)
		if !new_nodes.is_empty() {
			let relationships = if self.llm_enabled() {
				// Enhanced relationship discovery with optional AI for complex cases
				self.discover_relationships_with_ai_enhancement(&new_nodes).await?
			} else {
				// Fast rule-based relationship discovery only
				self.discover_relationships_efficiently(&new_nodes).await?
			};
			if !relationships.is_empty() {
				let mut graph = self.graph.write().await;
				graph.relationships.extend(relationships.clone());
				drop(graph);

				// Save the nodes and relationships
				let db_ops = DatabaseOperations::new(&self.store);
				db_ops.save_graph_incremental(&new_nodes, &relationships).await?;
			} else {
				// Save just the nodes
				let db_ops = DatabaseOperations::new(&self.store);
				db_ops.save_graph_incremental(&new_nodes, &[]).await?;
			}
		}

		// Update final state
		if let Some(state) = state {
			let mut state_guard = state.write();
			state_guard.status_message = format!("GraphRAG processing complete: {} files processed ({} skipped)", processed_count, skipped_count);
		} else {
			println!("GraphRAG: Processed {} files ({} skipped)", processed_count, skipped_count);
		}

		Ok(())
	}

	// Enhanced relationship discovery with optional AI for complex cases
	async fn discover_relationships_with_ai_enhancement(&self, new_files: &[CodeNode]) -> Result<Vec<CodeRelationship>> {
		if let Some(ref ai) = self.ai_enhancements {
			// Get all nodes for context
			let all_nodes = {
				let graph = self.graph.read().await;
				graph.nodes.values().cloned().collect::<Vec<CodeNode>>()
			};
			ai.discover_relationships_with_ai_enhancement(new_files, &all_nodes).await
		} else {
			// Fallback to efficient discovery without AI
			self.discover_relationships_efficiently(new_files).await
		}
	}

	// Discover relationships efficiently without AI for most cases
	async fn discover_relationships_efficiently(&self, new_files: &[CodeNode]) -> Result<Vec<CodeRelationship>> {
		// Get all nodes from the graph for relationship discovery
		let all_nodes = {
			let graph = self.graph.read().await;
			graph.nodes.values().cloned().collect::<Vec<CodeNode>>()
		};

		RelationshipDiscovery::discover_relationships_efficiently(new_files, &all_nodes).await
	}

	// Determine if a file is complex enough to benefit from AI analysis
	fn should_use_ai_for_description(&self, symbols: &[String], lines: u32, language: &str) -> bool {
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
	async fn extract_ai_description(&self, content_sample: &str, file_path: &str, language: &str, symbols: &[String]) -> Result<String> {
		if let Some(ref ai) = self.ai_enhancements {
			ai.extract_ai_description(content_sample, file_path, language, symbols).await
		} else {
			Err(anyhow::anyhow!("AI enhancements not available"))
		}
	}

	// Legacy method for backward compatibility - now uses efficient code block processing
	pub async fn process_code_blocks(&self, code_blocks: &[CodeBlock], state: Option<SharedState>) -> Result<()> {
		// Use the new efficient method that processes code blocks directly
		self.process_files_from_codeblocks(code_blocks, state).await
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
			let symbols_contain = node.symbols.iter().any(|s| s.to_lowercase().contains(&query_lower));

			// Use a lower threshold for semantic similarity (0.5 instead of 0.6)
			// OR include if the query is a substring of any important field
			if similarity > 0.5 || name_contains || kind_contains || desc_contains || symbols_contain {
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
		let results = similarities.into_iter()
			.map(|(_, node)| node)
			.collect();

		Ok(results)
	}

	// Search for nodes in database
	async fn search_nodes_in_database(&self, query: &str) -> Result<Vec<CodeNode>> {
		// Generate an embedding for the query
		let query_embedding = self.generate_embedding(query).await?;

		let db_ops = DatabaseOperations::new(&self.store);
		db_ops.search_nodes_in_database(&query_embedding, query).await
	}

	// Find paths between nodes in the graph
	pub async fn find_paths(&self, source_id: &str, target_id: &str, max_depth: usize) -> Result<Vec<Vec<String>>> {
		let graph = self.graph.read().await;

		// Ensure both nodes exist
		if !graph.nodes.contains_key(source_id) || !graph.nodes.contains_key(target_id) {
			return Ok(Vec::new());
		}

		// Build an adjacency list for easier traversal
		let mut adjacency_list: HashMap<String, Vec<String>> = HashMap::new();
		for rel in &graph.relationships {
			adjacency_list.entry(rel.source.clone())
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
}