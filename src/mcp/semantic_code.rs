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
use crate::indexer::{extract_file_signatures, signatures_to_markdown, NoindexWalker, PathUtils};
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

	/// Get the tool definition for view_signatures
	pub fn get_view_signatures_tool_definition() -> McpTool {
		McpTool {
			name: "view_signatures".to_string(),
			description: "Extract and view function signatures, class definitions, and other meaningful code structures from files. Shows method signatures, class definitions, interfaces, and other declarations without full implementation details. Perfect for getting an overview of code structure and available APIs. Output is always in markdown format.".to_string(),
			input_schema: json!({
				"type": "object",
				"properties": {
					"files": {
						"type": "array",
						"description": "Array of file paths or glob patterns to analyze for signatures. Examples: ['src/main.rs'], ['**/*.py'], ['src/**/*.ts', 'lib/**/*.js']",
						"items": {
							"type": "string",
							"description": "File path or glob pattern. Can be exact paths like 'src/main.rs' or patterns like '**/*.py' to match multiple files"
						},
						"minItems": 1,
						"maxItems": 100
					}
				},
				"required": ["files"],
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

	/// Execute the view_signatures tool
	pub async fn execute_view_signatures(&self, arguments: &Value) -> Result<String> {
		let files_array = arguments
			.get("files")
			.and_then(|v| v.as_array())
			.ok_or_else(|| anyhow::anyhow!("Missing required parameter 'files': must be an array of file paths or glob patterns"))?;

		// Validate files array
		if files_array.is_empty() {
			return Err(anyhow::anyhow!("Invalid files parameter: array must contain at least one file path or pattern"));
		}
		if files_array.len() > 100 {
			return Err(anyhow::anyhow!("Invalid files parameter: array must contain no more than 100 patterns"));
		}

		// Extract file patterns
		let mut file_patterns = Vec::new();
		for file_value in files_array {
			let pattern = file_value
				.as_str()
				.ok_or_else(|| anyhow::anyhow!("Invalid file pattern: all items in files array must be strings"))?;

			if pattern.trim().is_empty() {
				return Err(anyhow::anyhow!("Invalid file pattern: patterns cannot be empty"));
			}

			file_patterns.push(pattern.to_string());
		}

		if self.debug {
			eprintln!("Executing view_signatures: files={:?} in directory '{}'",
				file_patterns, self.working_directory.display());
		}

		// Change to the working directory for processing
		let _original_dir = std::env::current_dir()?;
		std::env::set_current_dir(&self.working_directory)?;

		// Get files matching patterns
		let mut matching_files = Vec::new();

		for pattern in &file_patterns {
			// Use glob pattern matching
			let glob_pattern = match globset::Glob::new(pattern) {
				Ok(g) => g.compile_matcher(),
				Err(e) => {
					std::env::set_current_dir(&_original_dir)?;
					return Err(anyhow::anyhow!("Invalid glob pattern '{}': {}", pattern, e));
				}
			};

			// Use NoindexWalker to respect both .gitignore and .noindex files while finding files
			let walker = NoindexWalker::create_walker(&self.working_directory).build();

			for result in walker {
				let entry = match result {
					Ok(entry) => entry,
					Err(_) => continue,
				};

				// Skip directories, only process files
				if !entry.file_type().is_some_and(|ft| ft.is_file()) {
					continue;
				}

				// See if this file matches our pattern
				let relative_path = PathUtils::to_relative_string(entry.path(), &self.working_directory);
				if glob_pattern.is_match(&relative_path) {
					matching_files.push(entry.path().to_path_buf());
				}
			}
		}

		if matching_files.is_empty() {
			std::env::set_current_dir(&_original_dir)?;
			return Ok("No matching files found for the specified patterns.".to_string());
		}

		// Extract signatures from matching files
		let signatures = match extract_file_signatures(&matching_files) {
			Ok(sigs) => sigs,
			Err(e) => {
				std::env::set_current_dir(&_original_dir)?;
				return Err(anyhow::anyhow!("Failed to extract signatures: {}", e));
			}
		};

		// Restore original directory
		std::env::set_current_dir(&_original_dir)?;

		// Always return markdown format
		let markdown = signatures_to_markdown(&signatures);
		Ok(markdown)
	}
}
