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
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::memory::{MemoryManager, MemoryType, MemoryQuery};
use crate::mcp::types::McpTool;

/// Memory tools provider
pub struct MemoryProvider {
	memory_manager: Arc<Mutex<MemoryManager>>,
	working_directory: std::path::PathBuf,
	debug: bool,
}

impl MemoryProvider {
	pub async fn new(config: &Config, working_directory: std::path::PathBuf, debug: bool) -> Option<Self> {
		match MemoryManager::new(config).await {
			Ok(manager) => Some(Self {
				memory_manager: Arc::new(Mutex::new(manager)),
				working_directory,
				debug,
			}),
			Err(e) => {
				if debug {
					eprintln!("Warning: Failed to initialize memory manager: {}", e);
				}
				None
			}
		}
	}

	/// Get all tool definitions for memory operations
	pub fn get_tool_definitions() -> Vec<McpTool> {
		vec![
			McpTool {
				name: "memorize".to_string(),
				description: "Store important information, insights, or context in memory for future reference.".to_string(),
				input_schema: json!({
					"type": "object",
					"properties": {
						"title": {
							"type": "string",
							"description": "Short, descriptive title for the memory",
							"minLength": 5,
							"maxLength": 200
						},
						"content": {
							"type": "string",
							"description": "Detailed content to remember – explanations, code snippets, insights, decisions, etc.",
							"minLength": 10,
							"maxLength": 10000
						},
						"memory_type": {
							"type": "string",
							"description": "Category of memory for better organization",
							"enum": ["code", "architecture", "bug_fix", "feature", "documentation", "user_preference", "decision", "learning", "configuration", "testing", "performance", "security", "insight"],
							"default": "code"
						},
						"importance": {
							"type": "number",
							"description": "Importance score from 0.0 to 1.0 (higher = more important for retention)",
							"minimum": 0.0,
							"maximum": 1.0,
							"default": 0.5
						},
						"tags": {
							"type": "array",
							"description": "Optional tags for categorization and easier searching",
							"items": {
								"type": "string"
							},
							"maxItems": 10
						},
						"related_files": {
							"type": "array",
							"description": "Optional file paths related to this memory",
							"items": {
								"type": "string"
							},
							"maxItems": 20
						}
					},
					"required": ["title", "content"],
					"additionalProperties": false
				}),
			},
			McpTool {
				name: "remember".to_string(),
				description: "Search and retrieve stored memories using semantic search. Find relevant past information, decisions, patterns, or context based on your query. ".to_string(),
				input_schema: json!({
					"type": "object",
					"properties": {
						"query": {
							"type": "string",
							"description": "What you want to remember or search for in stored memories",
							"minLength": 3,
							"maxLength": 500
						},
						"memory_types": {
							"type": "array",
							"description": "Optional filter by memory types",
							"items": {
								"type": "string",
								"enum": ["code", "architecture", "bug_fix", "feature", "documentation", "user_preference", "decision", "learning", "configuration", "testing", "performance", "security", "insight"]
							}
						},
						"tags": {
							"type": "array",
							"description": "Optional filter by tags",
							"items": {
								"type": "string"
							}
						},
						"related_files": {
							"type": "array",
							"description": "Optional filter by related files",
							"items": {
								"type": "string"
							}
						},
						"limit": {
							"type": "integer",
							"description": "Maximum number of memories to return",
							"minimum": 1,
							"maximum": 50,
							"default": 10
						}
					},
					"required": ["query"],
					"additionalProperties": false
				}),
			},
			McpTool {
				name: "forget".to_string(),
				description: "Permanently remove specific memories by ID or forget multiple memories matching certain criteria.".to_string(),
				input_schema: json!({
					"type": "object",
					"properties": {
						"memory_id": {
							"type": "string",
							"description": "Specific memory ID to forget (get this from remember results)"
						},
						"query": {
							"type": "string",
							"description": "Query to find memories to forget (alternative to memory_id)"
						},
						"memory_types": {
							"type": "array",
							"description": "Filter by memory types when using query",
							"items": {
								"type": "string",
								"enum": ["code", "architecture", "bug_fix", "feature", "documentation", "user_preference", "decision", "learning", "configuration", "testing", "performance", "security", "insight"]
							}
						},
						"tags": {
							"type": "array",
							"description": "Filter by tags when using query",
							"items": {
								"type": "string"
							}
						},
						"confirm": {
							"type": "boolean",
							"description": "Must be true to confirm deletion",
							"const": true
						}
					},
					"required": ["confirm"],
					"additionalProperties": false
				}),
			}
		]
	}

	/// Execute the memorize tool
	pub async fn execute_memorize(&self, arguments: &Value) -> Result<String> {
		let title = arguments
			.get("title")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing required parameter 'title'"))?;

		let content = arguments
			.get("content")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing required parameter 'content'"))?;

		// Validate lengths
		if title.len() < 5 || title.len() > 200 {
			return Err(anyhow::anyhow!("Title must be between 5 and 200 characters"));
		}
		if content.len() < 10 || content.len() > 10000 {
			return Err(anyhow::anyhow!("Content must be between 10 and 10000 characters"));
		}

		let memory_type_str = arguments
			.get("memory_type")
			.and_then(|v| v.as_str())
			.unwrap_or("code");

		let memory_type = MemoryType::from(memory_type_str.to_string());

		let importance = arguments
			.get("importance")
			.and_then(|v| v.as_f64())
			.map(|v| v as f32);

		let tags = arguments
			.get("tags")
			.and_then(|v| v.as_array())
			.map(|arr| {
				arr.iter()
					.filter_map(|v| v.as_str().map(|s| s.to_string()))
					.collect::<Vec<String>>()
			});

		let related_files = arguments
			.get("related_files")
			.and_then(|v| v.as_array())
			.map(|arr| {
				arr.iter()
					.filter_map(|v| v.as_str().map(|s| s.to_string()))
					.collect::<Vec<String>>()
			});

		if self.debug {
			eprintln!("Memorizing: title='{}', type='{:?}', importance={:?}",
				title, memory_type, importance);
		}

		// Change to working directory for Git context
		let _original_dir = std::env::current_dir()?;
		std::env::set_current_dir(&self.working_directory)?;

		let memory = {
			let mut manager = self.memory_manager.lock().await;
			manager.memorize(
				memory_type,
				title.to_string(),
				content.to_string(),
				importance,
				tags,
				related_files,
			).await?
		};

		// Restore original directory
		std::env::set_current_dir(&_original_dir)?;

		// Return compact JSON response for token efficiency
		let response = json!({
			"success": 1,
			"memory_id": memory.id,
			"message": "Memory stored successfully"
		});

		Ok(response.to_string())
	}

	/// Execute the remember tool
	pub async fn execute_remember(&self, arguments: &Value) -> Result<String> {
		let query = arguments
			.get("query")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing required parameter 'query'"))?;

		if query.len() < 3 || query.len() > 500 {
			return Err(anyhow::anyhow!("Query must be between 3 and 500 characters"));
		}

		// Parse memory types filter
		let memory_types = if let Some(types_array) = arguments.get("memory_types").and_then(|v| v.as_array()) {
			let types: Vec<MemoryType> = types_array
				.iter()
				.filter_map(|v| v.as_str())
				.map(|s| MemoryType::from(s.to_string()))
				.collect();
			if types.is_empty() { None } else { Some(types) }
		} else {
			None
		};

		// Parse tags filter
		let tags = if let Some(tags_array) = arguments.get("tags").and_then(|v| v.as_array()) {
			let tag_list: Vec<String> = tags_array
				.iter()
				.filter_map(|v| v.as_str().map(|s| s.to_string()))
				.collect();
			if tag_list.is_empty() { None } else { Some(tag_list) }
		} else {
			None
		};

		// Parse related files filter
		let related_files = if let Some(files_array) = arguments.get("related_files").and_then(|v| v.as_array()) {
			let file_list: Vec<String> = files_array
				.iter()
				.filter_map(|v| v.as_str().map(|s| s.to_string()))
				.collect();
			if file_list.is_empty() { None } else { Some(file_list) }
		} else {
			None
		};

		// Set limit
		let limit = arguments
			.get("limit")
			.and_then(|v| v.as_u64())
			.map(|v| v as usize)
			.unwrap_or(10);

		let memory_query = MemoryQuery {
			memory_types,
			tags,
			related_files,
			limit: Some(limit.min(50)),
			..Default::default()
		};

		if self.debug {
			eprintln!("Remembering: query='{}', limit={}", query, memory_query.limit.unwrap_or(10));
		}

		let results = {
			let manager = self.memory_manager.lock().await;
			manager.remember(query, Some(memory_query)).await?
		};

		if results.is_empty() {
			return Ok("# No Memories Found\n\nNo stored memories match your query. Try:\n- Using different search terms\n- Removing filters\n- Checking if any memories have been stored yet".to_string());
		}

		let mut output = format!("# Found {} Memories\n\n", results.len());

		for (i, result) in results.iter().enumerate() {
			output.push_str(&format!(
				"## Memory {} (Relevance: {:.2})\n\n\
					**ID:** {}\n\
					**Title:** {}\n\
					**Type:** {}\n\
					**Importance:** {:.2}\n\
					**Created:** {}\n\
					**Git Commit:** {}\n\
					**Tags:** {}\n\
					**Related Files:** {}\n\n\
					### Content\n{}\n\n\
					**Selection Reason:** {}\n\n",
				i + 1,
				result.relevance_score,
				result.memory.id,
				result.memory.title,
				result.memory.memory_type,
				result.memory.metadata.importance,
				result.memory.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
				result.memory.metadata.git_commit.as_deref().unwrap_or("None"),
				if result.memory.metadata.tags.is_empty() { "None".to_string() } else { result.memory.metadata.tags.join(", ") },
				if result.memory.metadata.related_files.is_empty() { "None".to_string() } else { result.memory.metadata.related_files.join(", ") },
				result.memory.content,
				result.selection_reason,
			));

			if i < results.len() - 1 {
				output.push_str("---\n\n");
			}
		}

		Ok(output)
	}

	/// Execute the forget tool
	pub async fn execute_forget(&self, arguments: &Value) -> Result<String> {
		// Check confirm parameter
		if !arguments.get("confirm").and_then(|v| v.as_bool()).unwrap_or(false) {
			return Ok(json!({
				"success": 0,
				"error": "Missing required confirmation: set 'confirm' to true to proceed with deletion"
			}).to_string());
		}

		// Handle specific memory ID deletion
		if let Some(memory_id) = arguments.get("memory_id").and_then(|v| v.as_str()) {
			if self.debug {
				eprintln!("Forgetting memory: id='{}'", memory_id);
			}

			let res = {
				let mut manager = self.memory_manager.lock().await;
				manager.forget(memory_id).await
			};
			match res {
				Ok(_) => Ok(json!({
					"success": 1,
					"memory_id": memory_id,
					"message": "Memory deleted successfully"
				}).to_string()),
				Err(e) => Ok(json!({
					"success": 0,
					"memory_id": memory_id,
					"error": format!("Failed to delete memory: {}", e)
			}).to_string())
			}
		}
		// Handle query-based deletion
		else if let Some(query) = arguments.get("query").and_then(|v| v.as_str()) {
			// Parse memory types filter
			let memory_types = if let Some(types_array) = arguments.get("memory_types").and_then(|v| v.as_array()) {
				let types: Vec<MemoryType> = types_array
					.iter()
					.filter_map(|v| v.as_str())
					.map(|s| MemoryType::from(s.to_string()))
					.collect();
				if types.is_empty() { None } else { Some(types) }
			} else {
				None
			};

			// Parse tags filter
			let tags = if let Some(tags_array) = arguments.get("tags").and_then(|v| v.as_array()) {
				let tag_list: Vec<String> = tags_array
					.iter()
					.filter_map(|v| v.as_str().map(|s| s.to_string()))
					.collect();
				if tag_list.is_empty() { None } else { Some(tag_list) }
			} else {
				None
			};

			let memory_query = MemoryQuery {
				query_text: Some(query.to_string()),
				memory_types,
				tags,
				..Default::default()
			};

			if self.debug {
				eprintln!("Forgetting memories matching: query='{}'", query);
			}

			let res = {
				let mut manager = self.memory_manager.lock().await;
				manager.forget_matching(memory_query).await
			};
			match res {
				Ok(deleted_count) => Ok(json!({
					"success": 1,
					"deleted_count": deleted_count,
					"message": format!("{} memories deleted successfully", deleted_count)
				}).to_string()),
				Err(e) => Ok(json!({
					"success": 0,
					"error": format!("Failed to delete memories: {}", e)
				}).to_string())
			}
		} else {
			Ok(json!({
				"success": 0,
				"error": "Either 'memory_id' or 'query' must be provided"
			}).to_string())
		}
	}
}
