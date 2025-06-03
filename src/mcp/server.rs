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
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::mcp::graphrag::GraphRagProvider;
use crate::mcp::memory::MemoryProvider;
use crate::mcp::semantic_code::SemanticCodeProvider;
use crate::mcp::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

/// MCP Server implementation with modular tool providers
pub struct McpServer {
	semantic_code: SemanticCodeProvider,
	graphrag: Option<GraphRagProvider>,
	memory: Option<MemoryProvider>,
	debug: bool,
	working_directory: std::path::PathBuf,
	watcher_handle: Option<tokio::task::JoinHandle<()>>,
}

impl McpServer {
	pub async fn new(
		config: Config,
		debug: bool,
		working_directory: std::path::PathBuf,
	) -> Result<Self> {
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
		})
	}

	pub async fn run(&mut self) -> Result<()> {
		// Start the file watcher
		self.start_watcher().await?;

		if self.debug {
			eprintln!("MCP Server started with debug mode");
		}

		let stdin = tokio::io::stdin();
		let stdout = tokio::io::stdout();
		let mut reader = BufReader::new(stdin);
		let mut writer = stdout;

		let mut line = String::new();

		loop {
			line.clear();
			match reader.read_line(&mut line).await {
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

		Ok(())
	}

	async fn start_watcher(&mut self) -> Result<()> {
		let (tx, mut rx) = mpsc::channel(100);
		let working_dir = self.working_directory.clone();

		// Start watcher in background
		let watcher_handle = tokio::spawn(async move {
			if let Err(e) = run_watcher(tx, working_dir).await {
				eprintln!("Watcher error: {}", e);
			}
		});

		// Handle watcher events
		let _index_handle = tokio::spawn(async move {
			while let Some(_event) = rx.recv().await {
				// Trigger reindex when files change
				if let Err(e) = trigger_reindex().await {
					eprintln!("Reindex error: {}", e);
				}
			}
		});

		self.watcher_handle = Some(watcher_handle);
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

// Helper functions
async fn run_watcher(tx: mpsc::Sender<()>, working_dir: std::path::PathBuf) -> Result<()> {
	use notify::RecursiveMode;
	use notify_debouncer_mini::{new_debouncer, DebouncedEvent};
	use std::time::Duration;

	let (debouncer_tx, mut debouncer_rx) = mpsc::channel(100);

	let mut debouncer = new_debouncer(
		Duration::from_millis(500),
		move |res: Result<Vec<DebouncedEvent>, notify::Error>| match res {
			Ok(_events) => {
				let _ = debouncer_tx.try_send(());
			}
			Err(e) => eprintln!("Watcher error: {:?}", e),
		},
	)?;

	debouncer
		.watcher()
		.watch(&working_dir, RecursiveMode::Recursive)?;

	while (debouncer_rx.recv().await).is_some() {
		let _ = tx.send(()).await;
	}

	Ok(())
}

async fn trigger_reindex() -> Result<()> {
	// Run indexing in background process to avoid blocking MCP server
	let mut child = Command::new(std::env::current_exe()?)
		.args(["index"])
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()?;

	// Don't wait for completion, let it run in background
	tokio::spawn(async move {
		let _ = child.wait().await;
	});

	Ok(())
}
