use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::path::PathBuf;

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

fn default_database_path() -> String {
    ".octocode/database.lance".to_string()
}

fn default_similarity_threshold() -> f32 {
    0.1
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum EmbeddingProvider {
    #[default]
    FastEmbed,
    Jina,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastEmbedConfig {
    #[serde(default = "default_embedding_model")]
    pub code_model: String,
    
    #[serde(default = "default_embedding_model")]
    pub text_model: String,
}

impl Default for FastEmbedConfig {
    fn default() -> Self {
        Self {
            code_model: "all-MiniLM-L6-v2".to_string(),
            text_model: "all-MiniLM-L6-v2".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JinaConfig {
    #[serde(default = "default_embedding_model")]
    pub code_model: String,
    
    #[serde(default = "default_embedding_model")]
    pub text_model: String,
}

impl Default for JinaConfig {
    fn default() -> Self {
        Self {
            code_model: "jina-embeddings-v3".to_string(),
            text_model: "jina-embeddings-v3".to_string(),
        }
    }
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
            ignore_patterns: vec![
                ".git/".to_string(),
                "target/".to_string(),
                "node_modules/".to_string(),
                ".octocode/".to_string(),
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_database_path")]
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_database_path(),
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
    
    #[serde(default)]
    pub database: DatabaseConfig,
    
    #[serde(default)]
    pub embedding_provider: EmbeddingProvider,
    
    #[serde(default)]
    pub fastembed: FastEmbedConfig,
    
    #[serde(default)]
    pub jina: JinaConfig,
    
    #[serde(default)]
    pub graphrag: GraphRAGConfig,
    
    pub jina_api_key: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_dir = Self::ensure_config_dir()?;
        let config_path = config_dir.join("config.toml");
        
        let mut config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            toml::from_str(&content)?
        } else {
            // Create default config if it doesn't exist
            let config = Config::default();
            let toml_content = toml::to_string_pretty(&config)?;
            fs::write(&config_path, toml_content)?;
            config
        };

        // Environment variables take precedence over config file values
        if let Ok(api_key) = std::env::var("OPENROUTER_API_KEY") {
            config.openrouter.api_key = Some(api_key);
        }
        if let Ok(jina_key) = std::env::var("JINA_API_KEY") {
            config.jina_api_key = Some(jina_key);
        }

        Ok(config)
    }
    
    pub fn save(&self) -> Result<()> {
        let config_dir = Self::ensure_config_dir()?;
        let config_path = config_dir.join("config.toml");
        
        let toml_content = toml::to_string_pretty(self)?;
        fs::write(config_path, toml_content)?;
        Ok(())
    }
    
    fn ensure_config_dir() -> Result<PathBuf> {
        let config_dir = std::env::current_dir()?.join(".octocode");
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)?;
        }
        Ok(config_dir)
    }
    
    pub fn get_database_path(&self) -> PathBuf {
        if Path::new(&self.database.path).is_absolute() {
            PathBuf::from(&self.database.path)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(&self.database.path)
        }
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
    }
}