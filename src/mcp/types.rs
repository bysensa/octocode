use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP Protocol types
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
	pub jsonrpc: String,
	pub id: Option<Value>,
	pub method: String,
	pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
	pub jsonrpc: String,
	pub id: Option<Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub result: Option<Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
	pub code: i32,
	pub message: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub data: Option<Value>,
}

/// MCP Tool definitions
#[derive(Debug, Serialize, Deserialize)]
pub struct McpTool {
	pub name: String,
	pub description: String,
	#[serde(rename = "inputSchema")]
	pub input_schema: Value,
}
