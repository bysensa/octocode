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

// GraphRAG AI-powered enhancements

use crate::config::Config;
use crate::indexer::graphrag::types::{CodeNode, CodeRelationship};
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

pub struct AIEnhancements {
	config: Config,
	client: Client,
}

impl AIEnhancements {
	pub fn new(config: Config, client: Client) -> Self {
		Self { config, client }
	}

	// Check if LLM enhancements are enabled
	pub fn llm_enabled(&self) -> bool {
		self.config.graphrag.use_llm
	}

	// Enhanced relationship discovery with optional AI for complex cases
	pub async fn discover_relationships_with_ai_enhancement(
		&self,
		new_files: &[CodeNode],
		all_nodes: &[CodeNode],
	) -> Result<Vec<CodeRelationship>> {
		// Start with rule-based relationships (fast and reliable)
		let mut relationships = crate::indexer::graphrag::relationships::RelationshipDiscovery::discover_relationships_efficiently(new_files, all_nodes).await?;

		// Add AI-enhanced relationship discovery for complex architectural patterns
		let ai_relationships = self
			.discover_complex_relationships_with_ai(new_files, all_nodes)
			.await?;
		relationships.extend(ai_relationships);

		// Deduplicate
		relationships.sort_by(|a, b| {
			(a.source.clone(), a.target.clone(), a.relation_type.clone()).cmp(&(
				b.source.clone(),
				b.target.clone(),
				b.relation_type.clone(),
			))
		});
		relationships.dedup_by(|a, b| {
			a.source == b.source && a.target == b.target && a.relation_type == b.relation_type
		});

		Ok(relationships)
	}

	// Use AI to discover complex architectural relationships
	async fn discover_complex_relationships_with_ai(
		&self,
		new_files: &[CodeNode],
		all_nodes: &[CodeNode],
	) -> Result<Vec<CodeRelationship>> {
		let mut ai_relationships = Vec::new();

		// Only use AI for files that are likely to have complex architectural relationships
		let complex_files: Vec<&CodeNode> = new_files
			.iter()
			.filter(|node| self.should_use_ai_for_relationships(node))
			.collect();

		if complex_files.is_empty() {
			return Ok(ai_relationships);
		}

		// Process in small batches to avoid overwhelming the AI
		const AI_BATCH_SIZE: usize = 3;
		for batch in complex_files.chunks(AI_BATCH_SIZE) {
			if let Ok(batch_relationships) = self
				.analyze_architectural_relationships_batch(batch, all_nodes)
				.await
			{
				ai_relationships.extend(batch_relationships);
			}
		}

		Ok(ai_relationships)
	}

	// Determine if a file is complex enough to benefit from AI relationship analysis
	fn should_use_ai_for_relationships(&self, node: &CodeNode) -> bool {
		// Use AI for relationship discovery on files that are architecturally significant
		let is_interface_heavy = node
			.symbols
			.iter()
			.any(|s| s.contains("interface_") || s.contains("trait_"));
		let is_config_or_setup = node
			.symbols
			.iter()
			.any(|s| s.contains("config") || s.contains("setup") || s.contains("init"));
		let is_core_module = node.path.contains("core")
			|| node.path.contains("lib")
			|| node.name == "main"
			|| node.name == "index";
		let has_many_exports = node.exports.len() > 5;
		let is_large_file = node.size_lines > 200;

		// Focus AI on files that are likely to have complex, non-obvious relationships
		(is_interface_heavy || is_config_or_setup || is_core_module)
			&& (has_many_exports || is_large_file)
	}

	// Analyze architectural relationships using AI in small batches
	async fn analyze_architectural_relationships_batch(
		&self,
		source_nodes: &[&CodeNode],
		all_nodes: &[CodeNode],
	) -> Result<Vec<CodeRelationship>> {
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
				node.symbols
					.iter()
					.take(8)
					.cloned()
					.collect::<Vec<_>>()
					.join(", "),
				node.exports
					.iter()
					.take(5)
					.cloned()
					.collect::<Vec<_>>()
					.join(", ")
			));
		}

		// Add relevant target nodes (potential relationship targets)
		batch_prompt.push_str("POTENTIAL RELATIONSHIP TARGETS:\n");
		let relevant_targets: Vec<&CodeNode> = all_nodes
			.iter()
			.filter(|n| source_nodes.iter().all(|s| s.id != n.id)) // Not source files
			.filter(|n| !n.exports.is_empty() || n.size_lines > 100) // Has exports or is substantial
			.take(10) // Limit context size
			.collect();

		for node in &relevant_targets {
			batch_prompt.push_str(&format!(
				"File: {}\nLanguage: {}\nExports: {}\n\n",
				node.path,
				node.language,
				node.exports
					.iter()
					.take(3)
					.cloned()
					.collect::<Vec<_>>()
					.join(", ")
			));
		}

		batch_prompt.push_str("JSON Response:");

		// Call AI with architectural analysis
		match self
			.call_llm(&self.config.graphrag.relationship_model, batch_prompt, None)
			.await
		{
			Ok(response) => {
				// Parse AI response
				if let Ok(ai_relationships) = self.parse_ai_architectural_relationships(&response) {
					// Filter and validate relationships
					let valid_relationships: Vec<CodeRelationship> = ai_relationships
						.into_iter()
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
			}
			Err(e) => {
				eprintln!("Warning: AI architectural analysis failed: {}", e);
				Ok(Vec::new())
			}
		}
	}

	// Parse AI response for architectural relationships
	fn parse_ai_architectural_relationships(
		&self,
		response: &str,
	) -> Result<Vec<CodeRelationship>> {
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
			let relationships = ai_rels
				.into_iter()
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

	// Determine if a file is complex enough to benefit from AI analysis
	pub fn should_use_ai_for_description(
		&self,
		symbols: &[String],
		lines: u32,
		language: &str,
	) -> bool {
		// Use AI for files that are likely to benefit from better understanding
		let function_count = symbols
			.iter()
			.filter(|s| s.contains("function_") || s.contains("method_"))
			.count();
		let class_count = symbols
			.iter()
			.filter(|s| s.contains("class_") || s.contains("struct_"))
			.count();
		let interface_count = symbols
			.iter()
			.filter(|s| s.contains("interface_") || s.contains("trait_"))
			.count();

		// AI is beneficial for:
		// 1. Large files (>100 lines) with complex structure
		// 2. Files with many functions/classes (>5 symbols)
		// 3. Configuration files that benefit from context understanding
		// 4. Core library/framework files
		// 5. Files with interfaces/traits (architectural significance)

		let is_large_complex = lines > 100 && (function_count + class_count) > 5;
		let is_config_file = symbols
			.iter()
			.any(|s| s.contains("config") || s.contains("setting"));
		let is_core_file = symbols
			.iter()
			.any(|s| s.contains("main") || s.contains("lib") || s.contains("core"));
		let has_architecture = interface_count > 0 || class_count > 3;
		let is_important_language = matches!(language, "rust" | "typescript" | "python" | "go");

		(is_large_complex || is_config_file || is_core_file || has_architecture)
			&& is_important_language
	}

	// Build a meaningful content sample for AI analysis (not full file content)
	pub fn build_content_sample_for_ai(&self, file_blocks: &[&crate::store::CodeBlock]) -> String {
		let mut sample = String::new();
		let mut total_chars = 0;
		const MAX_SAMPLE_SIZE: usize = 1500; // Reasonable size for AI context

		// Prioritize blocks with more symbols (more important code)
		let mut sorted_blocks: Vec<&crate::store::CodeBlock> = file_blocks.to_vec();
		sorted_blocks.sort_by(|a, b| b.symbols.len().cmp(&a.symbols.len()));

		for block in sorted_blocks {
			if total_chars >= MAX_SAMPLE_SIZE {
				break;
			}

			// Add block content with some context
			let block_content = if block.content.len() > 300 {
				// For large blocks, take beginning and end with proper UTF-8 handling
				let start_chars: String = block.content.chars().take(150).collect();
				let end_chars: String = block
					.content
					.chars()
					.rev()
					.take(150)
					.collect::<String>()
					.chars()
					.rev()
					.collect();
				format!("{}\n...\n{}", start_chars, end_chars)
			} else {
				block.content.clone()
			};

			sample.push_str(&format!(
				"// Block: {} symbols\n{}\n\n",
				block.symbols.len(),
				block_content
			));
			total_chars += block_content.len() + 50; // +50 for formatting
		}

		sample
	}

	// Extract AI-powered description for complex files
	pub async fn extract_ai_description(
		&self,
		content_sample: &str,
		file_path: &str,
		language: &str,
		symbols: &[String],
	) -> Result<String> {
		let function_count = symbols
			.iter()
			.filter(|s| s.contains("function_") || s.contains("method_"))
			.count();
		let class_count = symbols
			.iter()
			.filter(|s| s.contains("class_") || s.contains("struct_"))
			.count();

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

		match self
			.call_llm(&self.config.graphrag.description_model, prompt, None)
			.await
		{
			Ok(description) => {
				let cleaned = description.trim();
				if cleaned.len() > 300 {
					Ok(format!("{}...", &cleaned[0..297]))
				} else {
					Ok(cleaned.to_string())
				}
			}
			Err(e) => {
				eprintln!("Warning: AI description failed for {}: {}", file_path, e);
				Err(e)
			}
		}
	}

	// Call LLM API
	async fn call_llm(
		&self,
		model_name: &str,
		prompt: String,
		json_schema: Option<serde_json::Value>,
	) -> Result<String> {
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
		let response = self
			.client
			.post("https://openrouter.ai/api/v1/chat/completions")
			.header("Authorization", format!("Bearer {}", api_key))
			.header("HTTP-Referer", "https://github.com/muvon/octocode")
			.header("X-Title", "Octocode")
			.json(&request_body)
			.send()
			.await?;

		// Check if the API call was successful
		if !response.status().is_success() {
			let status = response.status();
			let error_text = response
				.text()
				.await
				.unwrap_or_else(|_| "Unable to read error response".to_string());
			return Err(anyhow::anyhow!("API error: {} - {}", status, error_text));
		}

		// Parse the response
		let response_json = response.json::<serde_json::Value>().await?;

		// Extract the response text
		if let Some(content) = response_json["choices"][0]["message"]["content"].as_str() {
			Ok(content.to_string())
		} else {
			// Provide more detailed error information
			Err(anyhow::anyhow!(
				"Failed to get response content: {:?}",
				response_json
			))
		}
	}
}
