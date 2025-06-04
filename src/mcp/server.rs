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

use anyhow::Result;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration, Instant};

use crate::config::Config;
use crate::indexer;
use crate::mcp::graphrag::GraphRagProvider;
use crate::mcp::memory::MemoryProvider;
use crate::mcp::semantic_code::SemanticCodeProvider;
use crate::mcp::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::state;
use crate::store::Store;
use crate::watcher_config::{
	IgnorePatterns, DEFAULT_ADDITIONAL_DELAY_MS, MCP_DEFAULT_DEBOUNCE_MS, MIN_DEBOUNCE_MS,
};

// Configurable debounce settings (code-only configuration for now)
//
// You can modify these constants to tune the MCP server behavior:
// - MCP_DEBOUNCE_MS: How long to wait after the last file change before triggering reindex
// - MCP_MAX_PENDING_EVENTS: Maximum number of file events to queue (prevents memory issues)
// - MCP_INDEX_TIMEOUT_MS: Maximum time to wait for indexing to complete before timing out
// - MCP_ENABLE_VERBOSE_EVENTS: Whether to log individual file events (useful for debugging)
//
const MCP_DEBOUNCE_MS: u64 = MCP_DEFAULT_DEBOUNCE_MS; // 2000ms = 2 seconds
const MCP_MAX_PENDING_EVENTS: usize = 100;
const MCP_INDEX_TIMEOUT_MS: u64 = 300_000; // 5 minutes
const MCP_ENABLE_VERBOSE_EVENTS: bool = false; // Set to true for detailed event logging

/// MCP Server implementation with modular tool providers
pub struct McpServer {
	semantic_code: SemanticCodeProvider,
	graphrag: Option<GraphRagProvider>,
	memory: Option<MemoryProvider>,
	debug: bool,
	working_directory: std::path::PathBuf,
	watcher_handle: Option<tokio::task::JoinHandle<()>>,
	index_handle: Option<tokio::task::JoinHandle<()>>,
	indexing_in_progress: Arc<AtomicBool>,
	store: Store,
	config: Config,
	index_rx: Option<mpsc::Receiver<()>>,
}

impl McpServer {
	pub async fn new(
		config: Config,
		debug: bool,
		working_directory: std::path::PathBuf,
	) -> Result<Self> {
		// Initialize the store for the MCP server
		let store = Store::new().await?;
		store.initialize_collections().await?;

		let semantic_code =
			SemanticCodeProvider::new(config.clone(), working_directory.clone(), debug);
		let graphrag = GraphRagProvider::new(config.clone(), working_directory.clone(), debug);
		let memory = MemoryProvider::new(&config, working_directory.clone(), debug).await;

		Ok(Self {
			semantic_code,
			graphrag,
			memory,
			debug,
			working_directory,
			watcher_handle: None,
			index_handle: None,
			indexing_in_progress: Arc::new(AtomicBool::new(false)),
			store,
			config,
			index_rx: None,
		})
	}

	pub async fn run(&mut self) -> Result<()> {
		// Start the file watcher
		self.start_watcher().await?;

		if self.debug {
			eprintln!("MCP Server started with debug mode");
			eprintln!(
				"Watch configuration: debounce={}ms, timeout={}ms, max_events={}",
				MCP_DEBOUNCE_MS, MCP_INDEX_TIMEOUT_MS, MCP_MAX_PENDING_EVENTS
			);
		}

		let stdin = tokio::io::stdin();
		let stdout = tokio::io::stdout();
		let mut reader = BufReader::new(stdin);
		let mut writer = stdout;

		let mut line = String::new();
		let mut index_rx = self.index_rx.take().unwrap();

		loop {
			line.clear();

			tokio::select! {
				// Handle stdin input (MCP requests)
				result = reader.read_line(&mut line) => {
					match result {
						Ok(0) => break, // EOF
						Ok(_) => {
							if let Some(response) = self.handle_request(&line).await {
								let response_json = serde_json::to_string(&response)?;
								writer.write_all(response_json.as_bytes()).await?;
								writer.write_all(b"\n").await?;
								writer.flush().await?;
							}
						}
						Err(e) => {
							if self.debug {
								eprintln!("Error reading from stdin: {}", e);
							}
							break;
						}
					}
				}

				// Handle indexing requests with timeout protection
				Some(_) = index_rx.recv() => {
					if self.debug {
						eprintln!("Processing index request...");
					}

					// Additional delay to ensure all file operations are complete
					sleep(Duration::from_millis(DEFAULT_ADDITIONAL_DELAY_MS)).await;

					// Perform direct indexing with timeout protection
					let indexing_result = tokio::time::timeout(
						Duration::from_millis(MCP_INDEX_TIMEOUT_MS),
						self.perform_direct_indexing()
					).await;

					match indexing_result {
						Ok(Ok(())) => {
							if self.debug {
								eprintln!("Reindex completed successfully");
							}
						}
						Ok(Err(e)) => {
							if self.debug {
								eprintln!("Reindex error: {}", e);
							}
						}
						Err(_) => {
							if self.debug {
								eprintln!("Reindex timed out after {}ms", MCP_INDEX_TIMEOUT_MS);
							}
						}
					}

					// Always reset the indexing flag, even on error/timeout
					self.indexing_in_progress.store(false, Ordering::SeqCst);
				}
			}
		}

		// Cleanup: abort background tasks
		if let Some(handle) = self.watcher_handle.take() {
			handle.abort();
		}
		if let Some(handle) = self.index_handle.take() {
			handle.abort();
		}

		if self.debug {
			eprintln!("MCP Server stopped");
		}

		Ok(())
	}

	async fn start_watcher(&mut self) -> Result<()> {
		let (file_tx, file_rx) = mpsc::channel(MCP_MAX_PENDING_EVENTS);
		let (index_tx, index_rx) = mpsc::channel(10);
		let working_dir = self.working_directory.clone();
		let debug = self.debug;

		// Start file watcher in background
		let watcher_handle = tokio::spawn(async move {
			if let Err(e) = run_watcher(file_tx, working_dir, debug).await {
				if debug {
					eprintln!("Watcher error: {}", e);
				}
			}
		});

		// Start improved debouncing handler that properly accumulates events
		let indexing_in_progress = self.indexing_in_progress.clone();
		let debug_mode = self.debug;
		let index_handle = tokio::spawn(async move {
			let mut file_rx = file_rx;
			let mut last_event_time = None::<Instant>;
			let mut pending_events = 0u32;

			loop {
				// Wait for either a file event or timeout
				let timeout_duration = Duration::from_millis(MCP_DEBOUNCE_MS);

				tokio::select! {
					// New file event received
					event_result = file_rx.recv() => {
						match event_result {
							Some(_) => {
								pending_events += 1;
								last_event_time = Some(Instant::now());

								if debug_mode {
									eprintln!("File change detected (total pending: {})", pending_events);
								}
							}
							None => {
								if debug_mode {
									eprintln!("File watcher channel closed, stopping debouncer");
								}
								break;
							}
						}
					}

					// Debounce timeout - check if we should trigger indexing
					_ = sleep(timeout_duration), if last_event_time.is_some() => {
						if let Some(last_time) = last_event_time {
							// Check if enough time has passed since the last event
							if last_time.elapsed() >= timeout_duration && pending_events > 0 {
								// Try to acquire indexing lock
								if indexing_in_progress
									.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
									.is_ok()
								{
									if debug_mode {
										eprintln!("Debounce period completed, requesting reindex for {} events...", pending_events);
									}

									// Send indexing request to main loop
									if let Err(_) = index_tx.send(()).await {
										if debug_mode {
											eprintln!("Failed to send index request - server may be shutting down");
										}
										indexing_in_progress.store(false, Ordering::SeqCst);
										break;
									}

									// Reset counters
									pending_events = 0;
									last_event_time = None;
								} else if debug_mode {
									eprintln!("Indexing already in progress, will retry after current indexing completes");
									// Don't reset counters, will retry later
								}
							}
						}
					}
				}
			}
		});

		// Store the index receiver for handling in the main loop
		self.index_rx = Some(index_rx);
		self.watcher_handle = Some(watcher_handle);
		self.index_handle = Some(index_handle);
		Ok(())
	}

	async fn handle_request(&self, line: &str) -> Option<JsonRpcResponse> {
		let line = line.trim();
		if line.is_empty() {
			return None;
		}

		if self.debug {
			eprintln!("Received request: {}", line);
		}

		let request: JsonRpcRequest = match serde_json::from_str(line) {
			Ok(req) => req,
			Err(e) => {
				return Some(JsonRpcResponse {
					jsonrpc: "2.0".to_string(),
					id: None,
					result: None,
					error: Some(JsonRpcError {
						code: -32700,
						message: format!("Parse error: {}", e),
						data: None,
					}),
				});
			}
		};

		let response = match request.method.as_str() {
			"initialize" => self.handle_initialize(&request).await,
			"tools/list" => self.handle_tools_list(&request).await,
			"tools/call" => self.handle_tools_call(&request).await,
			"ping" => self.handle_ping(&request).await,
			_ => JsonRpcResponse {
				jsonrpc: "2.0".to_string(),
				id: request.id,
				result: None,
				error: Some(JsonRpcError {
					code: -32601,
					message: "Method not found".to_string(),
					data: None,
				}),
			},
		};

		Some(response)
	}

	async fn perform_direct_indexing(&self) -> Result<()> {
		if self.debug {
			eprintln!("Starting direct reindex...");
		}

		// Create shared state for indexing (same as watch command)
		let state = state::create_shared_state();
		state.write().current_directory = self.working_directory.clone();

		// Get git root for optimization (same as watch command)
		let git_repo_root = indexer::git::find_git_root(&self.working_directory);

		// Perform the indexing directly (same as watch command in quiet mode)
		indexer::index_files(
			&self.store,
			state.clone(),
			&self.config,
			git_repo_root.as_deref(),
		)
		.await?;

		if self.debug {
			eprintln!("Direct reindex completed successfully");
		}

		Ok(())
	}

	async fn handle_initialize(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
		JsonRpcResponse {
			jsonrpc: "2.0".to_string(),
			id: request.id.clone(),
			result: Some(json!({
				"protocolVersion": "2024-11-05",
				"capabilities": {
					"tools": {
						"listChanged": false
					}
				},
				"serverInfo": {
					"name": "octocode-mcp",
					"version": "0.1.0",
					"description": "Semantic code search server with vector embeddings, memory system, and optional GraphRAG support"
				},
				"instructions": "This server provides modular AI tools: semantic code search, memory management, and GraphRAG. Use 'search_code' for code/documentation searches, memory tools for storing/retrieving context, and 'search_graphrag' (if available) for relationship queries."
			})),
			error: None,
		}
	}

	async fn handle_tools_list(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
		let mut tools = vec![
			SemanticCodeProvider::get_tool_definition(),
			SemanticCodeProvider::get_view_signatures_tool_definition(),
		];

		// Add memory tools if available
		if self.memory.is_some() {
			tools.extend(MemoryProvider::get_tool_definitions());
		}

		// Add GraphRAG tools if available
		if self.graphrag.is_some() {
			tools.push(GraphRagProvider::get_tool_definition());
		}

		JsonRpcResponse {
			jsonrpc: "2.0".to_string(),
			id: request.id.clone(),
			result: Some(json!({
				"tools": tools
			})),
			error: None,
		}
	}

	async fn handle_tools_call(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
		let params = match &request.params {
			Some(params) => params,
			None => {
				return JsonRpcResponse {
					jsonrpc: "2.0".to_string(),
					id: request.id.clone(),
					result: None,
					error: Some(JsonRpcError {
						code: -32602,
						message: "Invalid params: missing parameters object".to_string(),
						data: Some(json!({
							"details": "Tool calls require a 'params' object with 'name' and 'arguments' fields"
						})),
					}),
				};
			}
		};

		let tool_name = match params.get("name").and_then(|v| v.as_str()) {
			Some(name) => name,
			None => {
				return JsonRpcResponse {
					jsonrpc: "2.0".to_string(),
					id: request.id.clone(),
					result: None,
					error: Some(JsonRpcError {
						code: -32602,
						message: "Invalid params: missing tool name".to_string(),
						data: Some(json!({
							"details": "Required field 'name' must be provided with the tool name to call"
						})),
					}),
				};
			}
		};

		let default_args = json!({});
		let arguments = params.get("arguments").unwrap_or(&default_args);

		let result = match tool_name {
			"search_code" => self.semantic_code.execute_search(arguments).await,
			"view_signatures" => self.semantic_code.execute_view_signatures(arguments).await,
			"search_graphrag" => match &self.graphrag {
				Some(provider) => provider.execute_search(arguments).await,
				None => Err(anyhow::anyhow!("GraphRAG is not enabled in the current configuration. Please enable GraphRAG in octocode.toml to use relationship-aware search.")),
			},
			"memorize" => match &self.memory {
				Some(provider) => provider.execute_memorize(arguments).await,
				None => Err(anyhow::anyhow!("Memory system is not available")),
			},
			"remember" => match &self.memory {
				Some(provider) => provider.execute_remember(arguments).await,
				None => Err(anyhow::anyhow!("Memory system is not available")),
			},
			"forget" => match &self.memory {
				Some(provider) => provider.execute_forget(arguments).await,
				None => Err(anyhow::anyhow!("Memory system is not available")),
			},
			_ => Err(anyhow::anyhow!("Unknown tool '{}'. Available tools: search_code, view_signatures{}{}",
				tool_name,
				if self.graphrag.is_some() { ", search_graphrag" } else { "" },
				if self.memory.is_some() { ", memorize, remember, forget" } else { "" }
			)),
		};

		match result {
			Ok(content) => JsonRpcResponse {
				jsonrpc: "2.0".to_string(),
				id: request.id.clone(),
				result: Some(json!({
					"content": [{
						"type": "text",
						"text": content
					}]
				})),
				error: None,
			},
			Err(e) => {
				let error_message = e.to_string();
				let error_code =
					if error_message.contains("Missing") || error_message.contains("Invalid") {
						-32602 // Invalid params
					} else if error_message.contains("not enabled")
						|| error_message.contains("not available")
					{
						-32601 // Method not found (feature not available)
					} else {
						-32603 // Internal error
					};

				JsonRpcResponse {
					jsonrpc: "2.0".to_string(),
					id: request.id.clone(),
					result: None,
					error: Some(JsonRpcError {
						code: error_code,
						message: format!("Tool execution failed: {}", error_message),
						data: Some(json!({
							"tool": tool_name,
							"error_type": match error_code {
							-32602 => "invalid_params",
							-32601 => "feature_unavailable",
							_ => "execution_error"
						}
						})),
					}),
				}
			}
		}
	}

	async fn handle_ping(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
		JsonRpcResponse {
			jsonrpc: "2.0".to_string(),
			id: request.id.clone(),
			result: Some(json!({})),
			error: None,
		}
	}
}

impl Drop for McpServer {
	fn drop(&mut self) {
		// Ensure background tasks are cleaned up
		if let Some(handle) = self.watcher_handle.take() {
			handle.abort();
		}
		if let Some(handle) = self.index_handle.take() {
			handle.abort();
		}
	}
}

// Helper functions
async fn run_watcher(
	tx: mpsc::Sender<()>,
	working_dir: std::path::PathBuf,
	debug: bool,
) -> Result<()> {
	use notify::RecursiveMode;
	use notify_debouncer_mini::{new_debouncer, DebouncedEvent};

	let (debouncer_tx, mut debouncer_rx) = mpsc::channel(MCP_MAX_PENDING_EVENTS);

	// Create ignore patterns manager
	let ignore_patterns = IgnorePatterns::new(working_dir.clone());

	// Use minimal debounce for the file watcher itself - we handle the real debouncing in the event handler
	let mut debouncer = new_debouncer(
		Duration::from_millis(MIN_DEBOUNCE_MS),
		move |res: Result<Vec<DebouncedEvent>, notify::Error>| match res {
			Ok(events) => {
				// Filter out events from irrelevant paths using ignore patterns
				let relevant_events: Vec<_> = events
					.iter()
					.filter(|event| !ignore_patterns.should_ignore_path(&event.path))
					.collect();

				if !relevant_events.is_empty() {
					if debug && MCP_ENABLE_VERBOSE_EVENTS {
						eprintln!(
							"File watcher detected {} relevant events",
							relevant_events.len()
						);
						for event in &relevant_events {
							eprintln!("  - {:?}: {:?}", event.kind, event.path);
						}
					}

					// Send notification for each relevant event batch
					let _ = debouncer_tx.try_send(());
				}
			}
			Err(e) => {
				if debug {
					eprintln!("File watcher error: {:?}", e);
				}
			}
		},
	)?;

	debouncer
		.watcher()
		.watch(&working_dir, RecursiveMode::Recursive)?;

	if debug {
		eprintln!("File watcher started for: {}", working_dir.display());
		eprintln!("Loaded ignore patterns from .gitignore and .noindex files");
		eprintln!(
			"Using debounce settings: {}ms debounce, {}ms additional delay",
			MCP_DEBOUNCE_MS, DEFAULT_ADDITIONAL_DELAY_MS
		);
	}

	// Forward events from debouncer to the main event handler
	while let Some(_) = debouncer_rx.recv().await {
		if tx.send(()).await.is_err() {
			if debug {
				eprintln!("Event channel closed, stopping file watcher");
			}
			break;
		}
	}

	if debug {
		eprintln!("File watcher stopped");
	}

	Ok(())
}
