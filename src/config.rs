use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::embedding::types::EmbeddingProviderConfig;
use crate::storage;

// Default values functions
fn default_model() -> String {
	"openai/gpt-4.1-mini".to_string()
}

fn default_base_url() -> String {
	"https://openrouter.ai/api/v1".to_string()
}

fn default_timeout() -> u64 {
	120
}

fn default_output_format() -> String {
	"markdown".to_string()
}

fn default_max_files() -> usize {
	20
}

fn default_context_lines() -> usize {
	3
}

fn default_search_block_max_characters() -> usize {
	1000
}

fn default_chunk_size() -> usize {
	2000
}

fn default_chunk_overlap() -> usize {
	100
}

fn default_embedding_model() -> String {
	"all-MiniLM-L6-v2".to_string()
}

fn default_max_results() -> usize {
	50
}



fn default_similarity_threshold() -> f32 {
	0.6
}

fn default_top_k() -> usize {
	20
}

fn default_graphrag_enabled() -> bool {
	false
}

fn default_embeddings_batch_size() -> usize {
	32
}

// Embedding configuration defaults
fn default_embedding_config() -> EmbeddingProviderConfig {
	EmbeddingProviderConfig::get_default_models()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRAGConfig {
	#[serde(default = "default_graphrag_enabled")]
	pub enabled: bool,

	#[serde(default = "default_model")]
	pub description_model: String,

	#[serde(default = "default_model")]
	pub relationship_model: String,
}

impl Default for GraphRAGConfig {
	fn default() -> Self {
		Self {
			enabled: default_graphrag_enabled(),
			description_model: default_model(),
			relationship_model: default_model(),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterConfig {
	#[serde(default = "default_model")]
	pub model: String,

	#[serde(default = "default_base_url")]
	pub base_url: String,

	#[serde(default = "default_timeout")]
	pub timeout: u64,

	pub api_key: Option<String>,
}

impl Default for OpenRouterConfig {
	fn default() -> Self {
		Self {
			model: default_model(),
			base_url: default_base_url(),
			timeout: default_timeout(),
			api_key: None,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
	#[serde(default = "default_chunk_size")]
	pub chunk_size: usize,

	#[serde(default = "default_chunk_overlap")]
	pub chunk_overlap: usize,

	#[serde(default = "default_embedding_model")]
	pub embedding_model: String,

	#[serde(default = "default_embeddings_batch_size")]
	pub embeddings_batch_size: usize,

	#[serde(default = "default_graphrag_enabled")]
	pub graphrag_enabled: bool,

	#[serde(default = "default_graphrag_enabled")]
	pub llm_enabled: bool,

	#[serde(default)]
	pub ignore_patterns: Vec<String>,
}

impl Default for IndexConfig {
	fn default() -> Self {
		Self {
			chunk_size: default_chunk_size(),
			chunk_overlap: default_chunk_overlap(),
			embedding_model: default_embedding_model(),
			embeddings_batch_size: default_embeddings_batch_size(),
			graphrag_enabled: default_graphrag_enabled(),
			llm_enabled: default_graphrag_enabled(),
			ignore_patterns: vec![
				".git/".to_string(),
				"target/".to_string(),
				"node_modules/".to_string(),
			],
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
	#[serde(default = "default_max_results")]
	pub max_results: usize,

	#[serde(default = "default_similarity_threshold")]
	pub similarity_threshold: f32,

	#[serde(default = "default_top_k")]
	pub top_k: usize,

	#[serde(default = "default_output_format")]
	pub output_format: String,

	#[serde(default = "default_max_files")]
	pub max_files: usize,

	#[serde(default = "default_context_lines")]
	pub context_lines: usize,

	/// Maximum characters to display per code/text/doc block in search results.
	/// If 0, displays full content. Default: 1000
	#[serde(default = "default_search_block_max_characters")]
	pub search_block_max_characters: usize,
}

impl Default for SearchConfig {
	fn default() -> Self {
		Self {
			max_results: default_max_results(),
			similarity_threshold: default_similarity_threshold(),
			top_k: default_top_k(),
			output_format: default_output_format(),
			max_files: default_max_files(),
			context_lines: default_context_lines(),
			search_block_max_characters: default_search_block_max_characters(),
		}
	}
}



#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
	#[serde(default)]
	pub openrouter: OpenRouterConfig,

	#[serde(default)]
	pub index: IndexConfig,

	#[serde(default)]
	pub search: SearchConfig,

	#[serde(default = "default_embedding_config")]
	pub embedding: EmbeddingProviderConfig,

	#[serde(default)]
	pub graphrag: GraphRAGConfig,
}

impl Config {
	pub fn load() -> Result<Self> {
		let config_path = Self::get_system_config_path()?;

		let mut config = if config_path.exists() {
			let content = fs::read_to_string(&config_path)?;
			toml::from_str(&content)?
		} else {
			// Create default config if it doesn't exist
			let config = Config::default();
			let toml_content = toml::to_string_pretty(&config)?;
			
			// Ensure the parent directory exists
			if let Some(parent) = config_path.parent() {
				if !parent.exists() {
					fs::create_dir_all(parent)?;
				}
			}
			
			fs::write(&config_path, toml_content)?;
			config
		};

		// Environment variables take precedence over config file values
		if let Ok(api_key) = std::env::var("OPENROUTER_API_KEY") {
			config.openrouter.api_key = Some(api_key);
		}

		Ok(config)
	}

	pub fn save(&self) -> Result<()> {
		let config_path = Self::get_system_config_path()?;

		// Ensure the parent directory exists
		if let Some(parent) = config_path.parent() {
			if !parent.exists() {
				fs::create_dir_all(parent)?;
			}
		}

		let toml_content = toml::to_string_pretty(self)?;
		fs::write(config_path, toml_content)?;
		Ok(())
	}

	/// Get the system-wide config file path
	/// Stored at ~/.local/share/octocode/config.toml (same level as fastembed cache)
	fn get_system_config_path() -> Result<PathBuf> {
		let system_storage = storage::get_system_storage_dir()?;
		Ok(system_storage.join("config.toml"))
	}



	pub fn get_model(&self) -> &str {
		&self.openrouter.model
	}

	pub fn get_base_url(&self) -> &str {
		&self.openrouter.base_url
	}

	pub fn get_timeout(&self) -> u64 {
		self.openrouter.timeout
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_config() {
		let config = Config::default();
		assert_eq!(config.openrouter.model, "openai/gpt-4.1-mini");
		assert_eq!(config.index.chunk_size, 2000);
		assert_eq!(config.search.max_results, 50);
		assert_eq!(config.embedding.provider, crate::embedding::types::EmbeddingProviderType::FastEmbed);
	}
}
