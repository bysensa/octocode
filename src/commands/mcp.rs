use anyhow::Result;
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use octocode::config::Config;
use octocode::indexer::search::search_codebase;
use octocode::indexer::graphrag::GraphRAG;

#[derive(Args, Clone)]
pub struct McpArgs {
	/// Enable debug logging for MCP server
	#[arg(long)]
	pub debug: bool,

	/// Path to the directory to serve (defaults to current directory)
	#[arg(long, default_value = ".")]
	pub path: String,
}

// MCP Protocol types
#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
	jsonrpc: String,
	id: Option<Value>,
	method: String,
	params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
	jsonrpc: String,
	id: Option<Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	result: Option<Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError {
	code: i32,
	message: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	data: Option<Value>,
}

// MCP Tool definitions
#[derive(Debug, Serialize, Deserialize)]
struct McpTool {
	name: String,
	description: String,
	#[serde(rename = "inputSchema")]
	input_schema: Value,
}

// MCP Server implementation
pub struct McpServer {
	config: Config,
	graphrag: Option<GraphRAG>,
	debug: bool,
	working_directory: std::path::PathBuf,
	watcher_handle: Option<tokio::task::JoinHandle<()>>,
}

impl McpServer {
	pub fn new(config: Config, debug: bool, working_directory: std::path::PathBuf) -> Self {
		let graphrag = if config.graphrag.enabled {
			Some(GraphRAG::new(config.clone()))
		} else {
			None
		};

		Self {
			config,
			graphrag,
			debug,
			working_directory,
			watcher_handle: None,
		}
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
					"description": "Semantic code search server with vector embeddings and optional GraphRAG support"
				},
				"instructions": "This server provides semantic search capabilities for codebases. Use 'search_code' for general code/documentation searches, or 'search_graphrag' (if available) for complex relationship queries. All results are returned in markdown format with syntax highlighting."
			})),
			error: None,
		}
	}

	async fn handle_tools_list(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
		let mut tools = vec![
			McpTool {
				name: "search_code".to_string(),
				description: "Search through the codebase using semantic vector search to find relevant code snippets, functions, classes, documentation, or text content. Returns results formatted as markdown with file paths, line numbers, relevance scores, and syntax-highlighted code blocks.".to_string(),
				input_schema: json!({
					"type": "object",
					"properties": {
						"query": {
							"type": "string",
							"description": "Natural language search query describing what you're looking for. Examples: 'authentication functions', 'error handling code', 'database connection setup', 'API endpoints for user management', 'configuration parsing logic'",
							"minLength": 3,
							"maxLength": 500
						},
						"mode": {
							"type": "string",
							"description": "Scope of search to limit results to specific content types",
							"enum": ["code", "text", "docs", "all"],
							"default": "all",
							"enumDescriptions": {
								"code": "Search only in code blocks (functions, classes, methods, etc.)",
								"text": "Search only in plain text files and text content",
								"docs": "Search only in documentation files (README, markdown, etc.)",
								"all": "Search across all content types for comprehensive results"
							}
						}
					},
					"required": ["query"],
					"additionalProperties": false
				}),
			}
		];

		// Add GraphRAG tools if enabled
		if self.graphrag.is_some() {
			tools.push(McpTool {
				name: "search_graphrag".to_string(),
				description: "Advanced relationship-aware search using GraphRAG (Graph Retrieval-Augmented Generation). This tool understands code relationships, dependencies, and semantic connections between different parts of the codebase. Best for complex queries about how components interact, architectural patterns, or cross-cutting concerns.".to_string(),
				input_schema: json!({
					"type": "object",
					"properties": {
						"query": {
							"type": "string",
							"description": "Complex query about code relationships, architecture, or cross-cutting concerns. Examples: 'How does user authentication flow through the system?', 'What components depend on the database layer?', 'Show me the data flow for order processing', 'Find all error handling patterns across modules'",
							"minLength": 10,
							"maxLength": 1000
						}
					},
					"required": ["query"],
					"additionalProperties": false
				}),
			});
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
			"search_code" => self.execute_search_code(arguments).await,
			"search_graphrag" => self.execute_search_graphrag(arguments).await,
			_ => Err(anyhow::anyhow!("Unknown tool '{}'. Available tools: search_code{}",
				tool_name,
				if self.graphrag.is_some() { ", search_graphrag" } else { "" }
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
				let error_code = if error_message.contains("Missing") || error_message.contains("Invalid") {
					-32602 // Invalid params
				} else if error_message.contains("not enabled") {
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

	async fn execute_search_code(&self, arguments: &Value) -> Result<String> {
		let query = arguments
			.get("query")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing required parameter 'query': must be a non-empty string describing what to search for"))?;

		// Validate query length
		if query.len() < 3 {
			return Err(anyhow::anyhow!("Invalid query: must be at least 3 characters long"));
		}
		if query.len() > 500 {
			return Err(anyhow::anyhow!("Invalid query: must be no more than 500 characters long"));
		}

		let mode = arguments
			.get("mode")
			.and_then(|v| v.as_str())
			.unwrap_or("all");

		// Validate mode
		if !["code", "text", "docs", "all"].contains(&mode) {
			return Err(anyhow::anyhow!("Invalid mode '{}': must be one of 'code', 'text', 'docs', or 'all'", mode));
		}

		if self.debug {
			eprintln!("Executing search: query='{}', mode='{}' in directory '{}'",
				query, mode, self.working_directory.display());
		}

		// Change to the working directory for the search
		let _original_dir = std::env::current_dir()?;
		std::env::set_current_dir(&self.working_directory)?;

		// Use the search functionality from the existing codebase
		let results = search_codebase(query, mode, &self.config).await;

		// Restore original directory
		std::env::set_current_dir(&_original_dir)?;

		results
	}

	async fn execute_search_graphrag(&self, arguments: &Value) -> Result<String> {
		let query = arguments
			.get("query")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing required parameter 'query': must be a detailed question about code relationships or architecture"))?;

		// Validate query length for GraphRAG (should be more detailed)
		if query.len() < 10 {
			return Err(anyhow::anyhow!("Invalid GraphRAG query: must be at least 10 characters long and describe relationships or architecture"));
		}
		if query.len() > 1000 {
			return Err(anyhow::anyhow!("Invalid GraphRAG query: must be no more than 1000 characters long"));
		}

		match &self.graphrag {
			Some(graphrag) => {
				if self.debug {
					eprintln!("Executing GraphRAG search: query='{}' in directory '{}'",
						query, self.working_directory.display());
				}

				// Change to the working directory for the search
				let _original_dir = std::env::current_dir()?;
				std::env::set_current_dir(&self.working_directory)?;

				let results = graphrag.search(query).await;

				// Restore original directory
				std::env::set_current_dir(&_original_dir)?;

				let results = results?;
				Ok(format_graphrag_results_as_markdown(results))
			}
			None => Err(anyhow::anyhow!("GraphRAG is not enabled in the current configuration. Please enable GraphRAG in octodev.toml to use relationship-aware search.")),
		}
	}
}

// Helper functions
async fn run_watcher(tx: mpsc::Sender<()>, working_dir: std::path::PathBuf) -> Result<()> {
	use notify::RecursiveMode;
	use notify_debouncer_mini::{new_debouncer, DebouncedEvent};
	use std::time::Duration;

	let (debouncer_tx, mut debouncer_rx) = mpsc::channel(100);

	let mut debouncer = new_debouncer(Duration::from_millis(500), move |res: Result<Vec<DebouncedEvent>, notify::Error>| {
		match res {
			Ok(_events) => {
				let _ = debouncer_tx.try_send(());
			}
			Err(e) => eprintln!("Watcher error: {:?}", e),
		}
	})?;

	debouncer.watcher().watch(&working_dir, RecursiveMode::Recursive)?;

	while let Some(_) = debouncer_rx.recv().await {
		let _ = tx.send(()).await;
	}

	Ok(())
}

async fn trigger_reindex() -> Result<()> {
	// Run indexing in background process to avoid blocking MCP server
	let mut child = Command::new(std::env::current_exe()?)
		.args(&["index"])
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()?;

	// Don't wait for completion, let it run in background
	tokio::spawn(async move {
		let _ = child.wait().await;
	});

	Ok(())
}

fn format_graphrag_results_as_markdown(results: String) -> String {
	// GraphRAG results are already formatted as markdown
	results
}

pub async fn run(args: McpArgs) -> Result<()> {
	let config = Config::load()?;

	// Convert path to absolute PathBuf
	let working_directory = std::path::Path::new(&args.path).canonicalize()
		.map_err(|e| anyhow::anyhow!("Invalid path '{}': {}", args.path, e))?;

	// Verify the path exists and is a directory
	if !working_directory.is_dir() {
		return Err(anyhow::anyhow!("Path '{}' is not a directory", working_directory.display()));
	}

	if args.debug {
		eprintln!("MCP Server starting with working directory: {}", working_directory.display());
	}

	let mut server = McpServer::new(config, args.debug, working_directory);
	server.run().await
}
