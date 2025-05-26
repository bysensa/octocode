// GraphRAG module for Octodev
// Handles code relationship extraction and graph generation

use crate::config::Config;
use crate::store::{Store, CodeBlock};
use crate::indexer::embed::calculate_unique_content_hash;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::array::Array;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;
use std::sync::Arc;
use reqwest::Client;
use serde_json::json;
use fastembed::{TextEmbedding, EmbeddingModel, InitOptions};
use std::path::{Path, PathBuf};
use crate::state::SharedState;

// A node in the code graph - represents a file/module with efficient storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeNode {
	pub id: String,           // Relative path from project root (efficient storage)
	pub name: String,         // File name or module name
	pub kind: String,         // Type of the node (file, module, package, function)
	pub path: String,         // Relative file path from project root
	pub description: String,  // Description/summary of what the file/module does
	pub symbols: Vec<String>, // All symbols from this file (functions, classes, etc.)
	pub hash: String,         // Content hash to detect changes
	pub embedding: Vec<f32>,  // Vector embedding of the file content
	pub imports: Vec<String>, // List of imported modules (relative paths or external)
	pub exports: Vec<String>, // List of exported symbols
	pub functions: Vec<FunctionInfo>, // Function-level information for better granularity
	pub size_lines: u32,      // Number of lines in the file
	pub language: String,     // Programming language
}

// Function-level information for better granularity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
	pub name: String,         // Function name
	pub signature: String,    // Function signature
	pub start_line: u32,      // Starting line number
	pub end_line: u32,        // Ending line number
	pub calls: Vec<String>,   // Functions this function calls
	pub called_by: Vec<String>, // Functions that call this function
	pub parameters: Vec<String>, // Function parameters
	pub return_type: Option<String>, // Return type if available
}

// A relationship between code nodes - simplified and more efficient
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRelationship {
	pub source: String,      // Source node ID (relative path)
	pub target: String,      // Target node ID (relative path)
	pub relation_type: String, // Type: imports, calls, extends, implements, etc.
	pub description: String, // Brief description
	pub confidence: f32,     // Confidence score (0.0-1.0)
	pub weight: f32,         // Relationship strength/frequency
}

// The full code graph
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeGraph {
	pub nodes: HashMap<String, CodeNode>,
	pub relationships: Vec<CodeRelationship>,
}

// Note: Old AI prompts removed - using more targeted AI interactions now

// Helper struct for batch relationship analysis request
#[derive(Debug, Serialize, Deserialize)]
struct BatchRelationshipResult {
	source_id: String,
	target_id: String,
	relation_type: String,
	description: String,
	confidence: f32,
	exists: bool,
}

// Manages the creation and storage of the code graph with project-relative paths
pub struct GraphBuilder {
	config: Config,
	graph: Arc<RwLock<CodeGraph>>,
	client: Client,
	embedding_model: Arc<TextEmbedding>,
	store: Store,
	project_root: PathBuf,  // Project root for relative path calculations
}

impl GraphBuilder {
	pub async fn new(config: Config) -> Result<Self> {
		// Detect project root (look for common indicators)
		let project_root = Self::detect_project_root()?;
		
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
		let graph = Arc::new(RwLock::new(Self::load_graph(&store, &project_root).await?));

		Ok(Self {
			config,
			graph,
			client: Client::new(),
			embedding_model: Arc::new(model),
			store,
			project_root,
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

	// Detect project root by looking for common indicators
	fn detect_project_root() -> Result<PathBuf> {
		let current_dir = std::env::current_dir()?;
		let mut dir = current_dir.as_path();
		
		// Look for common project root indicators
		let indicators = [
			"Cargo.toml", "package.json", ".git", "pyproject.toml", 
			"go.mod", "pom.xml", "build.gradle", "composer.json"
		];
		
		loop {
			for indicator in &indicators {
				if dir.join(indicator).exists() {
					return Ok(dir.to_path_buf());
				}
			}
			
			match dir.parent() {
				Some(parent) => dir = parent,
				None => break,
			}
		}
		
		// Fallback to current directory if no indicators found
		Ok(current_dir)
	}

	// Convert absolute path to relative path from project root
	fn to_relative_path(&self, absolute_path: &str) -> Result<String> {
		let abs_path = PathBuf::from(absolute_path);
		let relative = abs_path.strip_prefix(&self.project_root)
			.map_err(|_| anyhow::anyhow!("Path {} is not within project root {}", 
				absolute_path, self.project_root.display()))?;
		
		Ok(relative.to_string_lossy().to_string())
	}

	// Generate an embedding for node content
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		let embeddings = self.embedding_model.embed(vec![text], None)?;
		if embeddings.is_empty() {
			return Err(anyhow::anyhow!("Failed to generate embedding"));
		}
		Ok(embeddings[0].clone())
	}

	// Load the existing graph from database
	async fn load_graph(store: &Store, _project_root: &Path) -> Result<CodeGraph> {
		let mut graph = CodeGraph::default();

		// Check if the tables exist
		if !store.tables_exist(&["graphrag_nodes", "graphrag_relationships"]).await? {
			return Ok(graph); // Return empty graph if tables don't exist
		}

		// Get vector dimension for embedding work
		let vector_dim = store.get_vector_dim();

		// Get all nodes
		let node_batch = store.search_graph_nodes(&vec![0.0; vector_dim], 10000).await?;
		if node_batch.num_rows() == 0 {
			return Ok(graph); // No nodes found
		}

		println!("Loading {} GraphRAG nodes from database...", node_batch.num_rows());

		// Process nodes
		let id_array = node_batch.column_by_name("id").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let name_array = node_batch.column_by_name("name").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let kind_array = node_batch.column_by_name("kind").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let path_array = node_batch.column_by_name("path").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let description_array = node_batch.column_by_name("description").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let symbols_array = node_batch.column_by_name("symbols").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let imports_array = node_batch.column_by_name("imports").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let exports_array = node_batch.column_by_name("exports").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let hash_array = node_batch.column_by_name("hash").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();

		// Get the embedding fixed size list array
		let embedding_array = node_batch.column_by_name("embedding").unwrap()
			.as_any()
			.downcast_ref::<arrow::array::FixedSizeListArray>()
			.unwrap();

		// Get the values of the embedding array
		let embedding_values = embedding_array.values()
			.as_any()
			.downcast_ref::<arrow::array::Float32Array>()
			.unwrap();

		// Process each row
		for i in 0..node_batch.num_rows() {
			let id = id_array.value(i).to_string();
			let name = name_array.value(i).to_string();
			let kind = kind_array.value(i).to_string();
			let path = path_array.value(i).to_string();
			let description = description_array.value(i).to_string();

			// Parse symbols JSON
			let symbols: Vec<String> = if symbols_array.is_null(i) {
				Vec::new()
			} else {
				serde_json::from_str(symbols_array.value(i)).unwrap_or_default()
			};

			// Parse imports JSON
			let imports: Vec<String> = if imports_array.is_null(i) {
				Vec::new()
			} else {
				serde_json::from_str(imports_array.value(i)).unwrap_or_default()
			};

			// Parse exports JSON
			let exports: Vec<String> = if exports_array.is_null(i) {
				Vec::new()
			} else {
				serde_json::from_str(exports_array.value(i)).unwrap_or_default()
			};

			let hash = hash_array.value(i).to_string();

			// Extract the embedding for this node
			let embedding_offset = i * embedding_array.value_length() as usize;
			let embedding_len = embedding_array.value_length() as usize;
			let mut embedding = Vec::with_capacity(embedding_len);

			for j in 0..embedding_len {
				embedding.push(embedding_values.value(embedding_offset + j));
			}

			// Create the node
			let node = CodeNode {
				id,
				name,
				kind,
				path,
				description,
				symbols,
				imports,
				exports,
				functions: Vec::new(), // Default empty for nodes loaded from old schema
				size_lines: 0, // Default for nodes loaded from old schema
				language: "unknown".to_string(), // Default for nodes loaded from old schema
				hash,
				embedding,
			};

			// Add to graph
			graph.nodes.insert(node.id.clone(), node);
		}

		// Load relationships
		let rel_batch = store.get_graph_relationships().await?;
		if rel_batch.num_rows() > 0 {
			println!("Loading {} GraphRAG relationships from database...", rel_batch.num_rows());

			// Process relationships
			let source_array = rel_batch.column_by_name("source").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
			let target_array = rel_batch.column_by_name("target").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
			let type_array = rel_batch.column_by_name("relation_type").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
			let desc_array = rel_batch.column_by_name("description").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
			let conf_array = rel_batch.column_by_name("confidence").unwrap().as_any().downcast_ref::<arrow::array::Float32Array>().unwrap();

			// Process each relationship
			for i in 0..rel_batch.num_rows() {
				let relationship = CodeRelationship {
					source: source_array.value(i).to_string(),
					target: target_array.value(i).to_string(),
					relation_type: type_array.value(i).to_string(),
					description: desc_array.value(i).to_string(),
					confidence: conf_array.value(i),
					weight: 1.0, // Default weight for legacy relationships
				};

				// Add to graph
				graph.relationships.push(relationship);
			}
		}

		if !graph.nodes.is_empty() {
			println!("Loaded GraphRAG knowledge graph with {} nodes and {} relationships",
				graph.nodes.len(), graph.relationships.len());
		}

		Ok(graph)
	}

	// Save the graph to database with support for incremental updates
	#[allow(dead_code)]
	async fn save_graph(&self) -> Result<()> {
		// Get a read lock on the graph
		let graph = self.graph.read().await;

		// Check if there are any nodes to save
		if graph.nodes.is_empty() {
			println!("No GraphRAG nodes to save to database.");
			return Ok(());
		}

		// First, save the nodes
		println!("Saving {} GraphRAG nodes to database...", graph.nodes.len());

		// Convert the nodes to a RecordBatch
		let nodes_batch = self.nodes_to_batch(&graph.nodes).await?;

		// Clear existing nodes first for a clean update
		self.store.clear_graph_nodes().await?;

		// Store the nodes in the database
		self.store.store_graph_nodes(nodes_batch).await?;

		// Now save the relationships if any exist
		if !graph.relationships.is_empty() {
			println!("Saving {} GraphRAG relationships to database...", graph.relationships.len());

			// Convert the relationships to a RecordBatch
			let rel_batch = self.relationships_to_batch(&graph.relationships).await?;

			// Clear existing relationships first for a clean update
			self.store.clear_graph_relationships().await?;

			// Store the relationships in the database
			self.store.store_graph_relationships(rel_batch).await?;
		}

		println!("GraphRAG knowledge graph saved to database successfully.");

		Ok(())
	}

	// Save just the newly added nodes and relationships in batches
	async fn save_graph_incremental(&self, new_nodes: &[CodeNode], new_relationships: &[CodeRelationship]) -> Result<()> {
		if new_nodes.is_empty() && new_relationships.is_empty() {
			// Nothing to save
			return Ok(());
		}

		// First, save any new nodes
		if !new_nodes.is_empty() {
			// Create a HashMap for the batch conversion function
			let nodes_map: HashMap<String, CodeNode> = new_nodes.iter()
				.map(|node| (node.id.clone(), node.clone()))
				.collect();

			// Convert just these nodes to a RecordBatch
			let nodes_batch = self.nodes_to_batch(&nodes_map).await?;

			// Store the nodes in the database (appending to existing data)
			self.store.store_graph_nodes(nodes_batch).await?;
		}

		// Now save any new relationships
		if !new_relationships.is_empty() {
			// Convert just these relationships to a RecordBatch
			let rel_batch = self.relationships_to_batch(new_relationships).await?;

			// Store the relationships in the database (appending to existing data)
			self.store.store_graph_relationships(rel_batch).await?;
		}

		Ok(())
	}

	// Convert nodes to a RecordBatch for database storage with updated schema
	async fn nodes_to_batch(&self, nodes: &HashMap<String, CodeNode>) -> Result<arrow::record_batch::RecordBatch> {
		// Get the vector dimension from the store
		let vector_dim = self.store.get_vector_dim();

		// Create updated schema with new fields
		let schema = Arc::new(Schema::new(vec![
			Field::new("id", DataType::Utf8, false),
			Field::new("name", DataType::Utf8, false),
			Field::new("kind", DataType::Utf8, false),
			Field::new("path", DataType::Utf8, false),
			Field::new("description", DataType::Utf8, false),
			Field::new("symbols", DataType::Utf8, true),  // JSON serialized
			Field::new("imports", DataType::Utf8, true),  // JSON serialized
			Field::new("exports", DataType::Utf8, true),  // JSON serialized
			Field::new("functions", DataType::Utf8, true), // JSON serialized
			Field::new("size_lines", DataType::UInt32, false),
			Field::new("language", DataType::Utf8, false),
			Field::new("hash", DataType::Utf8, false),
			Field::new(
				"embedding",
				DataType::FixedSizeList(
					Arc::new(Field::new("item", DataType::Float32, true)),
					vector_dim as i32,
				),
				true,
			),
		]));

		// Prepare arrays
		let nodes_vec: Vec<&CodeNode> = nodes.values().collect();
		if nodes_vec.is_empty() {
			return Err(anyhow::anyhow!("No nodes to convert to batch"));
		}

		let ids: Vec<&str> = nodes_vec.iter().map(|n| n.id.as_str()).collect();
		let names: Vec<&str> = nodes_vec.iter().map(|n| n.name.as_str()).collect();
		let kinds: Vec<&str> = nodes_vec.iter().map(|n| n.kind.as_str()).collect();
		let paths: Vec<&str> = nodes_vec.iter().map(|n| n.path.as_str()).collect();
		let descriptions: Vec<&str> = nodes_vec.iter().map(|n| n.description.as_str()).collect();
		let symbols: Vec<String> = nodes_vec.iter().map(|n| serde_json::to_string(&n.symbols).unwrap_or_default()).collect();
		let imports: Vec<String> = nodes_vec.iter().map(|n| serde_json::to_string(&n.imports).unwrap_or_default()).collect();
		let exports: Vec<String> = nodes_vec.iter().map(|n| serde_json::to_string(&n.exports).unwrap_or_default()).collect();
		let functions: Vec<String> = nodes_vec.iter().map(|n| serde_json::to_string(&n.functions).unwrap_or_default()).collect();
		let size_lines: Vec<u32> = nodes_vec.iter().map(|n| n.size_lines).collect();
		let languages: Vec<&str> = nodes_vec.iter().map(|n| n.language.as_str()).collect();
		let hashes: Vec<&str> = nodes_vec.iter().map(|n| n.hash.as_str()).collect();

		// Create the embedding fixed size list array
		let mut flattened_embeddings = Vec::with_capacity(nodes_vec.len() * vector_dim);
		for node in &nodes_vec {
			// Ensure embeddings have the correct dimension
			if node.embedding.len() != vector_dim {
				return Err(anyhow::anyhow!(
					"Node embedding has dimension {} but expected {}",
					node.embedding.len(), vector_dim
				));
			}
			flattened_embeddings.extend_from_slice(&node.embedding);
		}
		let values = arrow::array::Float32Array::from(flattened_embeddings);

		// Create the fixed size list array
		let embedding_array = arrow::array::FixedSizeListArray::new(
			Arc::new(Field::new("item", DataType::Float32, true)),
			vector_dim as i32,
			Arc::new(values),
			None, // No validity buffer - all values are valid
		);

		// Create record batch
		let batch = arrow::record_batch::RecordBatch::try_new(
			schema,
			vec![
				Arc::new(arrow::array::StringArray::from(ids)),
				Arc::new(arrow::array::StringArray::from(names)),
				Arc::new(arrow::array::StringArray::from(kinds)),
				Arc::new(arrow::array::StringArray::from(paths)),
				Arc::new(arrow::array::StringArray::from(descriptions)),
				Arc::new(arrow::array::StringArray::from(symbols)),
				Arc::new(arrow::array::StringArray::from(imports)),
				Arc::new(arrow::array::StringArray::from(exports)),
				Arc::new(arrow::array::StringArray::from(functions)),
				Arc::new(arrow::array::UInt32Array::from(size_lines)),
				Arc::new(arrow::array::StringArray::from(languages)),
				Arc::new(arrow::array::StringArray::from(hashes)),
				Arc::new(embedding_array),
			],
		)?;

		Ok(batch)
	}

	// Convert relationships to a RecordBatch for database storage
	async fn relationships_to_batch(&self, relationships: &[CodeRelationship]) -> Result<arrow::record_batch::RecordBatch> {
		// Create updated schema with weight field
		let schema = Arc::new(Schema::new(vec![
			Field::new("id", DataType::Utf8, false),
			Field::new("source", DataType::Utf8, false),
			Field::new("target", DataType::Utf8, false),
			Field::new("relation_type", DataType::Utf8, false),
			Field::new("description", DataType::Utf8, false),
			Field::new("confidence", DataType::Float32, false),
			Field::new("weight", DataType::Float32, false),
		]));

		// Generate unique IDs
		let ids: Vec<String> = relationships.iter().map(|_| uuid::Uuid::new_v4().to_string()).collect();
		let sources: Vec<&str> = relationships.iter().map(|r| r.source.as_str()).collect();
		let targets: Vec<&str> = relationships.iter().map(|r| r.target.as_str()).collect();
		let types: Vec<&str> = relationships.iter().map(|r| r.relation_type.as_str()).collect();
		let descriptions: Vec<&str> = relationships.iter().map(|r| r.description.as_str()).collect();
		let confidences: Vec<f32> = relationships.iter().map(|r| r.confidence).collect();
		let weights: Vec<f32> = relationships.iter().map(|r| r.weight).collect();

		// Create record batch
		let batch = arrow::record_batch::RecordBatch::try_new(
			schema,
			vec![
				Arc::new(arrow::array::StringArray::from(ids)),
				Arc::new(arrow::array::StringArray::from(sources)),
				Arc::new(arrow::array::StringArray::from(targets)),
				Arc::new(arrow::array::StringArray::from(types)),
				Arc::new(arrow::array::StringArray::from(descriptions)),
				Arc::new(arrow::array::Float32Array::from(confidences)),
				Arc::new(arrow::array::Float32Array::from(weights)),
			],
		)?;

		Ok(batch)
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
				let kind = self.determine_file_kind(&relative_path);

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
					if let Ok(functions) = self.extract_functions_from_block(block) {
						all_functions.extend(functions);
					}
				}

				let symbols: Vec<String> = all_symbols.into_iter().collect();

				// Efficiently extract imports and exports based on language and symbols
				let (imports, exports) = self.extract_imports_exports_efficient(&symbols, &language, &relative_path);

				// Generate description - use AI for complex files when enabled
				let description = if self.llm_enabled() && self.should_use_ai_for_description(&symbols, total_lines as u32, &language) {
					// Collect a meaningful content sample for AI analysis
					let content_sample = self.build_content_sample_for_ai(&file_blocks);
					self.extract_ai_description(&content_sample, &file_path, &language, &symbols).await
						.unwrap_or_else(|_| self.generate_simple_description(&file_name, &language, &symbols, total_lines as u32))
				} else {
					self.generate_simple_description(&file_name, &language, &symbols, total_lines as u32)
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
				self.save_graph_incremental(&new_nodes, &relationships).await?;
			} else {
				// Save just the nodes
				self.save_graph_incremental(&new_nodes, &[]).await?;
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

	// Determine file kind based on path patterns
	fn determine_file_kind(&self, relative_path: &str) -> String {
		if relative_path.contains("/src/") || relative_path.contains("/lib/") {
			"source_file".to_string()
		} else if relative_path.contains("/test") || relative_path.contains("_test.") || relative_path.contains(".test.") {
			"test_file".to_string()
		} else if relative_path.ends_with(".md") || relative_path.ends_with(".txt") || relative_path.ends_with(".rst") {
			"documentation".to_string()
		} else if relative_path.contains("/config") || relative_path.contains(".config") {
			"config_file".to_string()
		} else if relative_path.contains("/examples") || relative_path.contains("/demo") {
			"example_file".to_string()
		} else {
			"file".to_string()
		}
	}

	// Extract function information from a code block efficiently
	fn extract_functions_from_block(&self, block: &CodeBlock) -> Result<Vec<FunctionInfo>> {
		let mut functions = Vec::new();
		
		// Look for function patterns in symbols
		for symbol in &block.symbols {
			if symbol.contains("function_") || symbol.contains("method_") {
				// Parse the symbol to extract function info
				if let Some(function_info) = self.parse_function_symbol(symbol, block) {
					functions.push(function_info);
				}
			}
		}
		
		Ok(functions)
	}

	// Parse function symbol to create FunctionInfo
	fn parse_function_symbol(&self, symbol: &str, block: &CodeBlock) -> Option<FunctionInfo> {
		// Simple pattern matching for common function symbol formats
		// This can be expanded based on your language implementations
		
		symbol.strip_prefix("function_").map(|function_name| FunctionInfo {
			name: function_name.to_string(),
			signature: format!("{}(...)", function_name), // Simplified
			start_line: block.start_line as u32,
			end_line: block.end_line as u32,
			calls: Vec::new(), // Will be populated during relationship discovery
			called_by: Vec::new(),
			parameters: Vec::new(), // Could be extracted from content if needed
			return_type: None,
		})
	}

	// Extract imports/exports efficiently based on language patterns and symbols
	fn extract_imports_exports_efficient(&self, symbols: &[String], language: &str, _relative_path: &str) -> (Vec<String>, Vec<String>) {
		let mut imports = Vec::new();
		let mut exports = Vec::new();

		// Use symbol patterns to determine imports/exports without re-parsing
		for symbol in symbols {
			if symbol.contains("import_") {
				if let Some(import_name) = symbol.strip_prefix("import_") {
					imports.push(import_name.to_string());
				}
			}
			
			if symbol.contains("export_") || symbol.contains("public_") {
				if let Some(export_name) = symbol.strip_prefix("export_").or_else(|| symbol.strip_prefix("public_")) {
					exports.push(export_name.to_string());
				}
			}
		}

		// Language-specific patterns
		match language {
			"rust" => {
				// For Rust, look for typical patterns
				for symbol in symbols {
					if symbol.starts_with("use_") {
						imports.push(symbol.strip_prefix("use_").unwrap_or(symbol).to_string());
					}
					if symbol.starts_with("pub_") {
						exports.push(symbol.strip_prefix("pub_").unwrap_or(symbol).to_string());
					}
				}
			},
			"javascript" | "typescript" => {
				// For JS/TS, look for module patterns
				for symbol in symbols {
					if symbol.contains("require_") || symbol.contains("from_") {
						imports.push(symbol.to_string());
					}
					if symbol.contains("module_exports") || symbol.contains("export_") {
						exports.push(symbol.to_string());
					}
				}
			},
			"python" => {
				// For Python, look for import patterns
				for symbol in symbols {
					if symbol.contains("import_") || symbol.contains("from_") {
						imports.push(symbol.to_string());
					}
					// In Python, most top-level symbols are exports
					if symbol.contains("function_") || symbol.contains("class_") {
						exports.push(symbol.to_string());
					}
				}
			},
			_ => {
				// Generic approach
			}
		}

		// Deduplicate
		imports.sort();
		imports.dedup();
		exports.sort();
		exports.dedup();

		(imports, exports)
	}

	// Determine if a file is complex enough to benefit from AI analysis
	fn should_use_ai_for_description(&self, symbols: &[String], lines: u32, language: &str) -> bool {
		// Use AI for files that are likely to benefit from better understanding
		let function_count = symbols.iter().filter(|s| s.contains("function_") || s.contains("method_")).count();
		let class_count = symbols.iter().filter(|s| s.contains("class_") || s.contains("struct_")).count();
		let interface_count = symbols.iter().filter(|s| s.contains("interface_") || s.contains("trait_")).count();
		
		// AI is beneficial for:
		// 1. Large files (>100 lines) with complex structure
		// 2. Files with many functions/classes (>5 symbols)
		// 3. Configuration files that benefit from context understanding
		// 4. Core library/framework files
		// 5. Files with interfaces/traits (architectural significance)
		
		let is_large_complex = lines > 100 && (function_count + class_count) > 5;
		let is_config_file = symbols.iter().any(|s| s.contains("config") || s.contains("setting"));
		let is_core_file = symbols.iter().any(|s| s.contains("main") || s.contains("lib") || s.contains("core"));
		let has_architecture = interface_count > 0 || class_count > 3;
		let is_important_language = matches!(language, "rust" | "typescript" | "python" | "go");
		
		(is_large_complex || is_config_file || is_core_file || has_architecture) && is_important_language
	}

	// Build a meaningful content sample for AI analysis (not full file content)
	fn build_content_sample_for_ai(&self, file_blocks: &[&CodeBlock]) -> String {
		let mut sample = String::new();
		let mut total_chars = 0;
		const MAX_SAMPLE_SIZE: usize = 1500; // Reasonable size for AI context
		
		// Prioritize blocks with more symbols (more important code)
		let mut sorted_blocks: Vec<&CodeBlock> = file_blocks.to_vec();
		sorted_blocks.sort_by(|a, b| b.symbols.len().cmp(&a.symbols.len()));
		
		for block in sorted_blocks {
			if total_chars >= MAX_SAMPLE_SIZE {
				break;
			}
			
			// Add block content with some context
			let block_content = if block.content.len() > 300 {
				// For large blocks, take beginning and end
				format!("{}\n...\n{}", 
					&block.content[0..150], 
					&block.content[block.content.len()-150..])
			} else {
				block.content.clone()
			};
			
			sample.push_str(&format!("// Block: {} symbols\n{}\n\n", block.symbols.len(), block_content));
			total_chars += block_content.len() + 50; // +50 for formatting
		}
		
		sample
	}

	// Extract AI-powered description for complex files
	async fn extract_ai_description(&self, content_sample: &str, file_path: &str, language: &str, symbols: &[String]) -> Result<String> {
		let function_count = symbols.iter().filter(|s| s.contains("function_") || s.contains("method_")).count();
		let class_count = symbols.iter().filter(|s| s.contains("class_") || s.contains("struct_")).count();
		
		let prompt = format!(
			"Analyze this {} file and provide a concise 2-3 sentence description focusing on its ROLE and PURPOSE in the codebase.\n\
			Focus on what this file accomplishes, its architectural significance, and how it fits into the larger system.\n\
			Avoid listing specific functions/classes - instead describe the file's overall responsibility.\n\n\
			File: {}\n\
			Language: {}\n\
			Stats: {} functions, {} classes/structs\n\
			Key symbols: {}\n\n\
			Code sample:\n{}\n\n\
			Description:",
			language,
			std::path::Path::new(file_path).file_name().and_then(|s| s.to_str()).unwrap_or("unknown"),
			language,
			function_count,
			class_count,
			symbols.iter().take(5).cloned().collect::<Vec<_>>().join(", "),
			content_sample
		);

		match self.call_llm(&self.config.graphrag.description_model, prompt, None).await {
			Ok(description) => {
				let cleaned = description.trim();
				if cleaned.len() > 300 {
					Ok(format!("{}...", &cleaned[0..297]))
				} else {
					Ok(cleaned.to_string())
				}
			},
			Err(e) => {
				eprintln!("Warning: AI description failed for {}: {}", file_path, e);
				Err(e)
			}
		}
	}

	// Generate simple description without AI for speed (fallback and default)
	fn generate_simple_description(&self, file_name: &str, language: &str, symbols: &[String], lines: u32) -> String {
		let function_count = symbols.iter().filter(|s| s.contains("function_") || s.contains("method_")).count();
		let class_count = symbols.iter().filter(|s| s.contains("class_") || s.contains("struct_")).count();
		
		if function_count > 0 && class_count > 0 {
			format!("{} {} file with {} functions and {} classes ({} lines)", 
				file_name, language, function_count, class_count, lines)
		} else if function_count > 0 {
			format!("{} {} file with {} functions ({} lines)", 
				file_name, language, function_count, lines)
		} else if class_count > 0 {
			format!("{} {} file with {} classes ({} lines)", 
				file_name, language, class_count, lines)
		} else {
			format!("{} {} file ({} lines)", file_name, language, lines)
		}
	}

	// Legacy method for backward compatibility - now uses efficient code block processing
	pub async fn process_code_blocks(&self, code_blocks: &[CodeBlock], state: Option<SharedState>) -> Result<()> {
		// Use the new efficient method that processes code blocks directly
		self.process_files_from_codeblocks(code_blocks, state).await
	}

	// Enhanced relationship discovery with optional AI for complex cases
	async fn discover_relationships_with_ai_enhancement(&self, new_files: &[CodeNode]) -> Result<Vec<CodeRelationship>> {
		// Start with rule-based relationships (fast and reliable)
		let mut relationships = self.discover_relationships_efficiently(new_files).await?;
		
		// Add AI-enhanced relationship discovery for complex architectural patterns
		let ai_relationships = self.discover_complex_relationships_with_ai(new_files).await?;
		relationships.extend(ai_relationships);
		
		// Deduplicate
		relationships.sort_by(|a, b| {
			(a.source.clone(), a.target.clone(), a.relation_type.clone())
				.cmp(&(b.source.clone(), b.target.clone(), b.relation_type.clone()))
		});
		relationships.dedup_by(|a, b| {
			a.source == b.source && a.target == b.target && a.relation_type == b.relation_type
		});
		
		Ok(relationships)
	}

	// Use AI to discover complex architectural relationships
	async fn discover_complex_relationships_with_ai(&self, new_files: &[CodeNode]) -> Result<Vec<CodeRelationship>> {
		let mut ai_relationships = Vec::new();
		
		// Get all nodes for context
		let all_nodes = {
			let graph = self.graph.read().await;
			graph.nodes.values().cloned().collect::<Vec<CodeNode>>()
		};
		
		// Only use AI for files that are likely to have complex architectural relationships
		let complex_files: Vec<&CodeNode> = new_files.iter()
			.filter(|node| self.should_use_ai_for_relationships(node))
			.collect();
		
		if complex_files.is_empty() {
			return Ok(ai_relationships);
		}
		
		// Process in small batches to avoid overwhelming the AI
		const AI_BATCH_SIZE: usize = 3;
		for batch in complex_files.chunks(AI_BATCH_SIZE) {
			if let Ok(batch_relationships) = self.analyze_architectural_relationships_batch(batch, &all_nodes).await {
				ai_relationships.extend(batch_relationships);
			}
		}
		
		Ok(ai_relationships)
	}

	// Determine if a file is complex enough to benefit from AI relationship analysis
	fn should_use_ai_for_relationships(&self, node: &CodeNode) -> bool {
		// Use AI for relationship discovery on files that are architecturally significant
		let is_interface_heavy = node.symbols.iter().any(|s| s.contains("interface_") || s.contains("trait_"));
		let is_config_or_setup = node.symbols.iter().any(|s| s.contains("config") || s.contains("setup") || s.contains("init"));
		let is_core_module = node.path.contains("core") || node.path.contains("lib") || node.name == "main" || node.name == "index";
		let has_many_exports = node.exports.len() > 5;
		let is_large_file = node.size_lines > 200;
		
		// Focus AI on files that are likely to have complex, non-obvious relationships
		(is_interface_heavy || is_config_or_setup || is_core_module) && (has_many_exports || is_large_file)
	}

	// Analyze architectural relationships using AI in small batches
	async fn analyze_architectural_relationships_batch(&self, source_nodes: &[&CodeNode], all_nodes: &[CodeNode]) -> Result<Vec<CodeRelationship>> {
		let mut batch_prompt = String::from(
			"You are an expert software architect. Analyze these code files and identify ARCHITECTURAL relationships.\n\
			Focus on design patterns, dependency injection, factory patterns, observer patterns, etc.\n\
			Look for relationships that go beyond simple imports - identify architectural significance.\n\n\
			Respond with a JSON array of relationships. For each relationship, include:\n\
			- source_path: relative path of source file\n\
			- target_path: relative path of target file\n\
			- relation_type: one of 'implements_pattern', 'dependency_injection', 'factory_creates', 'observer_pattern', 'strategy_pattern', 'adapter_pattern', 'decorator_pattern', 'architectural_dependency'\n\
			- description: brief explanation of the architectural relationship\n\
			- confidence: 0.0-1.0 confidence score\n\n"
		);

		// Add source nodes context
		batch_prompt.push_str("SOURCE FILES TO ANALYZE:\n");
		for node in source_nodes {
			batch_prompt.push_str(&format!(
				"File: {}\nLanguage: {}\nKey symbols: {}\nExports: {}\n\n",
				node.path,
				node.language,
				node.symbols.iter().take(8).cloned().collect::<Vec<_>>().join(", "),
				node.exports.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
			));
		}

		// Add relevant target nodes (potential relationship targets)
		batch_prompt.push_str("POTENTIAL RELATIONSHIP TARGETS:\n");
		let relevant_targets: Vec<&CodeNode> = all_nodes.iter()
			.filter(|n| source_nodes.iter().all(|s| s.id != n.id)) // Not source files
			.filter(|n| !n.exports.is_empty() || n.size_lines > 100) // Has exports or is substantial
			.take(10) // Limit context size
			.collect();

		for node in &relevant_targets {
			batch_prompt.push_str(&format!(
				"File: {}\nLanguage: {}\nExports: {}\n\n",
				node.path,
				node.language,
				node.exports.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
			));
		}

		batch_prompt.push_str("JSON Response:");

		// Call AI with architectural analysis
		match self.call_llm(&self.config.graphrag.relationship_model, batch_prompt, None).await {
			Ok(response) => {
				// Parse AI response
				if let Ok(ai_relationships) = self.parse_ai_architectural_relationships(&response) {
					// Filter and validate relationships
					let valid_relationships: Vec<CodeRelationship> = ai_relationships.into_iter()
						.filter(|rel| rel.confidence > 0.7) // Only high-confidence architectural relationships
						.filter(|rel| all_nodes.iter().any(|n| n.path == rel.target)) // Ensure target exists
						.map(|mut rel| {
							rel.weight = 0.9; // High weight for architectural relationships
							rel
						})
						.collect();
					
					Ok(valid_relationships)
				} else {
					Ok(Vec::new())
				}
			},
			Err(e) => {
				eprintln!("Warning: AI architectural analysis failed: {}", e);
				Ok(Vec::new())
			}
		}
	}

	// Parse AI response for architectural relationships
	fn parse_ai_architectural_relationships(&self, response: &str) -> Result<Vec<CodeRelationship>> {
		#[derive(Deserialize)]
		struct AiRelationship {
			source_path: String,
			target_path: String,
			relation_type: String,
			description: String,
			confidence: f32,
		}

		// Try to parse as JSON array
		if let Ok(ai_rels) = serde_json::from_str::<Vec<AiRelationship>>(response) {
			let relationships = ai_rels.into_iter()
				.map(|ai_rel| CodeRelationship {
					source: ai_rel.source_path,
					target: ai_rel.target_path,
					relation_type: ai_rel.relation_type,
					description: ai_rel.description,
					confidence: ai_rel.confidence,
					weight: 0.9, // High weight for AI-discovered architectural patterns
				})
				.collect();
			return Ok(relationships);
		}

		// If JSON parsing fails, return empty (AI might have responded in wrong format)
		Ok(Vec::new())
	}

	// Discover relationships efficiently without AI for most cases
	async fn discover_relationships_efficiently(&self, new_files: &[CodeNode]) -> Result<Vec<CodeRelationship>> {
		let mut relationships = Vec::new();

		// Get all nodes from the graph for relationship discovery
		let all_nodes = {
			let graph = self.graph.read().await;
			graph.nodes.values().cloned().collect::<Vec<CodeNode>>()
		};

		for source_file in new_files {
			// 1. Import/Export relationships (high confidence)
			for import in &source_file.imports {
				for target_file in &all_nodes {
					if target_file.id == source_file.id {
						continue;
					}
					
					// Check if target exports what source imports
					if target_file.exports.iter().any(|exp| self.symbols_match(import, exp)) ||
					   target_file.symbols.iter().any(|sym| self.symbols_match(import, sym)) {
						relationships.push(CodeRelationship {
							source: source_file.id.clone(),
							target: target_file.id.clone(),
							relation_type: "imports".to_string(),
							description: format!("Imports {} from {}", import, target_file.name),
							confidence: 0.9,
							weight: 1.0,
						});
					}
				}
			}

			// 2. Directory-based relationships (medium confidence)
			let source_dir = Path::new(&source_file.path).parent()
				.map(|p| p.to_string_lossy().to_string())
				.unwrap_or_else(|| ".".to_string());

			for other_file in &all_nodes {
				if other_file.id == source_file.id {
					continue;
				}

				let other_dir = Path::new(&other_file.path).parent()
					.map(|p| p.to_string_lossy().to_string())
					.unwrap_or_else(|| ".".to_string());

				// Same directory relationship
				if source_dir == other_dir && source_file.language == other_file.language {
					relationships.push(CodeRelationship {
						source: source_file.id.clone(),
						target: other_file.id.clone(),
						relation_type: "sibling_module".to_string(),
						description: format!("Same directory: {}", source_dir),
						confidence: 0.6,
						weight: 0.5,
					});
				}
			}

			// 3. Hierarchical module relationships (high confidence)
			for other_file in &all_nodes {
				if other_file.id == source_file.id {
					continue;
				}

				// Check for parent-child relationships based on path structure
				if self.is_parent_child_relationship(&source_file.path, &other_file.path) {
					let (parent, child) = if source_file.path.len() < other_file.path.len() {
						(&source_file.id, &other_file.id)
					} else {
						(&other_file.id, &source_file.id)
					};

					relationships.push(CodeRelationship {
						source: parent.clone(),
						target: child.clone(),
						relation_type: "contains".to_string(),
						description: "Hierarchical module relationship".to_string(),
						confidence: 0.8,
						weight: 0.7,
					});
				}
			}

			// 4. Language-specific pattern relationships
			self.discover_language_specific_relationships(source_file, &all_nodes, &mut relationships);
		}

		// Deduplicate relationships
		relationships.sort_by(|a, b| {
			(a.source.clone(), a.target.clone(), a.relation_type.clone())
				.cmp(&(b.source.clone(), b.target.clone(), b.relation_type.clone()))
		});
		relationships.dedup_by(|a, b| {
			a.source == b.source && a.target == b.target && a.relation_type == b.relation_type
		});

		Ok(relationships)
	}

	// Check if two symbols match (accounting for common patterns)
	fn symbols_match(&self, import: &str, export: &str) -> bool {
		// Direct match
		if import == export {
			return true;
		}
		
		// Clean symbol names (remove prefixes/suffixes)
		let clean_import = import.trim_start_matches("import_")
			.trim_start_matches("use_")
			.trim_start_matches("from_");
		let clean_export = export.trim_start_matches("export_")
			.trim_start_matches("pub_")
			.trim_start_matches("public_");
		
		clean_import == clean_export
	}

	// Check if paths have parent-child relationship
	fn is_parent_child_relationship(&self, path1: &str, path2: &str) -> bool {
		let path1_parts: Vec<&str> = path1.split('/').collect();
		let path2_parts: Vec<&str> = path2.split('/').collect();
		
		// One should be exactly one level deeper than the other
		if path1_parts.len().abs_diff(path2_parts.len()) == 1 {
			let (shorter, longer) = if path1_parts.len() < path2_parts.len() {
				(path1_parts, path2_parts)
			} else {
				(path2_parts, path1_parts)
			};
			
			// Check if all parts of shorter path match the beginning of longer path
			shorter.iter().zip(longer.iter()).all(|(a, b)| a == b)
		} else {
			false
		}
	}

	// Discover language-specific relationships
	fn discover_language_specific_relationships(&self, source_file: &CodeNode, all_nodes: &[CodeNode], relationships: &mut Vec<CodeRelationship>) {
		match source_file.language.as_str() {
			"rust" => {
				// Rust-specific patterns
				for other_file in all_nodes {
					if other_file.id == source_file.id || other_file.language != "rust" {
						continue;
					}
					
					// Check for mod.rs patterns
					if source_file.name == "mod" && other_file.path.starts_with(&source_file.path.replace("/mod.rs", "/")) {
						relationships.push(CodeRelationship {
							source: source_file.id.clone(),
							target: other_file.id.clone(),
							relation_type: "mod_declaration".to_string(),
							description: "Rust module declaration".to_string(),
							confidence: 0.8,
							weight: 0.8,
						});
					}
					
					// Check for lib.rs patterns
					if source_file.name == "lib" || source_file.name == "main" {
						let source_dir = Path::new(&source_file.path).parent()
							.map(|p| p.to_string_lossy().to_string())
							.unwrap_or_default();
						if other_file.path.starts_with(&source_dir) {
							relationships.push(CodeRelationship {
								source: source_file.id.clone(),
								target: other_file.id.clone(),
								relation_type: "crate_root".to_string(),
								description: "Rust crate root relationship".to_string(),
								confidence: 0.7,
								weight: 0.6,
							});
						}
					}
				}
			},
			"javascript" | "typescript" => {
				// JS/TS-specific patterns
				for other_file in all_nodes {
					if other_file.id == source_file.id || !["javascript", "typescript"].contains(&other_file.language.as_str()) {
						continue;
					}
					
					// Check for index.js patterns
					if source_file.name == "index" {
						let source_dir = Path::new(&source_file.path).parent()
							.map(|p| p.to_string_lossy().to_string())
							.unwrap_or_default();
						if other_file.path.starts_with(&source_dir) && other_file.name != "index" {
							relationships.push(CodeRelationship {
								source: source_file.id.clone(),
								target: other_file.id.clone(),
								relation_type: "index_module".to_string(),
								description: "JavaScript index module relationship".to_string(),
								confidence: 0.7,
								weight: 0.6,
							});
						}
					}
				}
			},
			"python" => {
				// Python-specific patterns
				for other_file in all_nodes {
					if other_file.id == source_file.id || other_file.language != "python" {
						continue;
					}
					
					// Check for __init__.py patterns
					if source_file.name == "__init__" {
						let source_dir = Path::new(&source_file.path).parent()
							.map(|p| p.to_string_lossy().to_string())
							.unwrap_or_default();
						if other_file.path.starts_with(&source_dir) && other_file.name != "__init__" {
							relationships.push(CodeRelationship {
								source: source_file.id.clone(),
								target: other_file.id.clone(),
								relation_type: "package_init".to_string(),
								description: "Python package initialization".to_string(),
								confidence: 0.8,
								weight: 0.7,
							});
						}
					}
				}
			},
			_ => {
				// Generic patterns for other languages
			}
		}
	}

	async fn call_llm(&self, model_name: &str, prompt: String, json_schema: Option<serde_json::Value>) -> Result<String> {
		// Check if we have an API key configured
		let api_key = match &self.config.openrouter.api_key {
			Some(key) => key.clone(),
			None => return Err(anyhow::anyhow!("OpenRouter API key not configured")),
		};

		// Prepare request body
		let mut request_body = json!({
		"model": model_name,
		"messages": [{
		"role": "user",
		"content": prompt
	}],
		// "max_tokens": 200
	});

		// Only add response_format if schema is provided
		if let Some(schema_value) = json_schema {
			request_body["response_format"] = json!({
				"type": "json_schema",
				"json_schema": {
					"name": "relationship",
					"strict": true,
					"schema": schema_value
				}
			});
		}


		// Call OpenRouter API
		let response = self.client
			.post("https://openrouter.ai/api/v1/chat/completions")
			.header("Authorization", format!("Bearer {}", api_key))
			.header("HTTP-Referer", "https://github.com/muvon/octodev")
			.header("X-Title", "Octodev")
			.json(&request_body)
			.send()
		.await?;

		// Check if the API call was successful
		if !response.status().is_success() {
			let status = response.status();
			let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
			return Err(anyhow::anyhow!("API error: {} - {}", status, error_text));
		}

		// Parse the response
		let response_json = response.json::<serde_json::Value>().await?;

		// Extract the response text
		if let Some(content) = response_json["choices"][0]["message"]["content"].as_str() {
			Ok(content.to_string())
		} else {
			// Provide more detailed error information
			Err(anyhow::anyhow!("Failed to get response content: {:?}", response_json))
		}
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

		// Check if the tables exist
		if !self.store.tables_exist(&["graphrag_nodes"]).await? {
			return Ok(Vec::new()); // No nodes in database
		}

		// Search for nodes similar to the query (increased limit to 50)
		let node_batch = self.store.search_graph_nodes(&query_embedding, 50).await?;
		if node_batch.num_rows() == 0 {
			return Ok(Vec::new());
		}

		// Process the results into CodeNode objects
		let mut nodes = Vec::new();
		let query_lower = query.to_lowercase();

		// Extract columns from the batch
		let id_array = node_batch.column_by_name("id").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let name_array = node_batch.column_by_name("name").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let kind_array = node_batch.column_by_name("kind").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let path_array = node_batch.column_by_name("path").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let description_array = node_batch.column_by_name("description").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let symbols_array = node_batch.column_by_name("symbols").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
		let hash_array = node_batch.column_by_name("hash").unwrap().as_any().downcast_ref::<arrow::array::StringArray>().unwrap();

		// Get the embedding fixed size list array
		let embedding_array = node_batch.column_by_name("embedding").unwrap()
			.as_any()
			.downcast_ref::<arrow::array::FixedSizeListArray>()
			.unwrap();

		// Get the values of the embedding array
		let embedding_values = embedding_array.values()
			.as_any()
			.downcast_ref::<arrow::array::Float32Array>()
			.unwrap();

		// Process each row
		for i in 0..node_batch.num_rows() {
			let id = id_array.value(i).to_string();
			let name = name_array.value(i).to_string();
			let kind = kind_array.value(i).to_string();
			let path = path_array.value(i).to_string();
			let description = description_array.value(i).to_string();

			// Parse symbols JSON
			let symbols: Vec<String> = if symbols_array.is_null(i) {
				Vec::new()
			} else {
				serde_json::from_str(symbols_array.value(i)).unwrap_or_default()
			};

			let hash = hash_array.value(i).to_string();

			// Extract the embedding for this node
			let embedding_offset = i * embedding_array.value_length() as usize;
			let embedding_len = embedding_array.value_length() as usize;
			let mut embedding = Vec::with_capacity(embedding_len);

			for j in 0..embedding_len {
				embedding.push(embedding_values.value(embedding_offset + j));
			}

			// Calculate semantic similarity
			let similarity = cosine_similarity(&query_embedding, &embedding);

			// Check if the query is a substring of various node fields
			let name_contains = name.to_lowercase().contains(&query_lower);
			let kind_contains = kind.to_lowercase().contains(&query_lower);
			let desc_contains = description.to_lowercase().contains(&query_lower);
			let symbols_contain = symbols.iter().any(|s| s.to_lowercase().contains(&query_lower));

			// Use a lower threshold for semantic similarity (0.5 instead of 0.6)
			// OR include if the query is a substring of any important field
			if similarity > 0.5 || name_contains || kind_contains || desc_contains || symbols_contain {
				// Create the node
				let node = CodeNode {
					id,
					name,
					kind,
					path,
					description,
					symbols,
					imports: Vec::new(), // Default empty for nodes loaded from old schema
					exports: Vec::new(), // Default empty for nodes loaded from old schema
					functions: Vec::new(), // Default empty for nodes loaded from old schema
					size_lines: 0, // Default for nodes loaded from old schema
					language: "unknown".to_string(), // Default for nodes loaded from old schema
					hash,
					embedding,
				};

				// Add to results
				nodes.push(node);
			}
		}

		// Sort nodes by relevance (exact matches first, then by similarity)
		nodes.sort_by(|a, b| {
			let a_contains = a.name.to_lowercase().contains(&query_lower) ||
			a.kind.to_lowercase().contains(&query_lower) ||
			a.symbols.iter().any(|s| s.to_lowercase().contains(&query_lower));

			let b_contains = b.name.to_lowercase().contains(&query_lower) ||
			b.kind.to_lowercase().contains(&query_lower) ||
			b.symbols.iter().any(|s| s.to_lowercase().contains(&query_lower));

			if a_contains && !b_contains {
				std::cmp::Ordering::Less
			} else if !a_contains && b_contains {
				return std::cmp::Ordering::Greater;
			} else {
				// Both contain or both don't contain, sort by similarity
				let a_sim = cosine_similarity(&query_embedding, &a.embedding);
				let b_sim = cosine_similarity(&query_embedding, &b.embedding);
				return b_sim.partial_cmp(&a_sim).unwrap_or(std::cmp::Ordering::Equal);
			}
		});

		Ok(nodes)
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

// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
	if a.len() != b.len() {
		return 0.0;
	}

	let mut dot_product = 0.0;
	let mut a_norm = 0.0;
	let mut b_norm = 0.0;

	for i in 0..a.len() {
		dot_product += a[i] * b[i];
		a_norm += a[i] * a[i];
		b_norm += b[i] * b[i];
	}

	a_norm = a_norm.sqrt();
	b_norm = b_norm.sqrt();

	if a_norm == 0.0 || b_norm == 0.0 {
		return 0.0;
	}

	dot_product / (a_norm * b_norm)
}

// Render GraphRAG nodes to JSON format
pub fn render_graphrag_nodes_json(nodes: &[CodeNode]) -> Result<(), anyhow::Error> {
	let json = serde_json::to_string_pretty(nodes)?;
	println!("{}", json);
	Ok(())
}

// Render GraphRAG nodes to Markdown format
pub fn graphrag_nodes_to_markdown(nodes: &[CodeNode]) -> String {
	let mut markdown = String::new();

	if nodes.is_empty() {
		markdown.push_str("No matching nodes found.");
		return markdown;
	}

	markdown.push_str(&format!("# Found {} GraphRAG nodes\n\n", nodes.len()));

	// Group nodes by file path for better organization
	let mut nodes_by_file: std::collections::HashMap<String, Vec<&CodeNode>> = std::collections::HashMap::new();

	for node in nodes {
		nodes_by_file
			.entry(node.path.clone())
			.or_default()
			.push(node);
	}

	// Print results organized by file
	for (file_path, file_nodes) in nodes_by_file.iter() {
		markdown.push_str(&format!("## File: {}\n\n", file_path));

		for node in file_nodes {
			markdown.push_str(&format!("### {} `{}`\n", node.kind, node.name));
			markdown.push_str(&format!("**ID:** {}  \n", node.id));
			markdown.push_str(&format!("**Description:** {}  \n", node.description));

			if !node.symbols.is_empty() {
				markdown.push_str("**Symbols:**  \n");
				// Display symbols
				let mut display_symbols = node.symbols.clone();
				display_symbols.sort();
				display_symbols.dedup();

				for symbol in display_symbols {
					// Only show non-type symbols to users
					if !symbol.contains("_") {
						markdown.push_str(&format!("- `{}`  \n", symbol));
					}
				}
			}

			markdown.push('\n');
		}

		markdown.push_str("---\n\n");
	}

	markdown
}

// GraphRAG implementation for searching
pub struct GraphRAG {
	config: Config,
}

impl GraphRAG {
	pub fn new(config: Config) -> Self {
		Self { config }
	}

	pub async fn search(&self, query: &str) -> Result<String> {
		let builder = GraphBuilder::new(self.config.clone()).await?;
		let nodes = builder.search_nodes(query).await?;
		Ok(graphrag_nodes_to_markdown(&nodes))
	}
}

// Helper functions for parsing imports and exports

