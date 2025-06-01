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

use crate::config::Config;
use crate::indexer::search::search_codebase;
use crate::mcp::types::McpTool;

/// Semantic code search tool provider
pub struct SemanticCodeProvider {
	config: Config,
	working_directory: std::path::PathBuf,
	debug: bool,
}

impl SemanticCodeProvider {
	pub fn new(config: Config, working_directory: std::path::PathBuf, debug: bool) -> Self {
		Self {
			config,
			working_directory,
			debug,
		}
	}

	/// Get the tool definition for search_code
	pub fn get_tool_definition() -> McpTool {
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
	}

	/// Execute the search_code tool
	pub async fn execute_search(&self, arguments: &Value) -> Result<String> {
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
}
