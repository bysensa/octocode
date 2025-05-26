use serde::{Deserialize, Serialize};

/// Supported embedding providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProviderType {
    FastEmbed,
    Jina,
    Voyage,
    Google,
}

impl Default for EmbeddingProviderType {
    fn default() -> Self {
        Self::FastEmbed
    }
}

/// Configuration for embedding models per content type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelConfig {
    /// Model for code content
    pub code_model: String,
    /// Model for text/documentation content
    pub text_model: String,
}

impl Default for EmbeddingModelConfig {
    fn default() -> Self {
        Self {
            code_model: "all-MiniLM-L6-v2".to_string(),
            text_model: "all-MiniLM-L6-v2".to_string(),
        }
    }
}

/// Provider-specific configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmbeddingProviderConfig {
    /// Currently active provider
    #[serde(default)]
    pub provider: EmbeddingProviderType,

    /// FastEmbed models configuration
    #[serde(default)]
    pub fastembed: EmbeddingModelConfig,

    /// Jina models configuration  
    #[serde(default)]
    pub jina: EmbeddingModelConfig,

    /// Voyage models configuration
    #[serde(default)]
    pub voyage: EmbeddingModelConfig,

    /// Google models configuration
    #[serde(default)]
    pub google: EmbeddingModelConfig,
}

impl EmbeddingProviderConfig {
    /// Get the model name for a specific provider and content type
    pub fn get_model(&self, provider: &EmbeddingProviderType, is_code: bool) -> &str {
        let config = match provider {
            EmbeddingProviderType::FastEmbed => &self.fastembed,
            EmbeddingProviderType::Jina => &self.jina,
            EmbeddingProviderType::Voyage => &self.voyage,
            EmbeddingProviderType::Google => &self.google,
        };

        if is_code {
            &config.code_model
        } else {
            &config.text_model
        }
    }

    /// Get the vector dimension for a specific provider and model
    pub fn get_vector_dimension(&self, provider: &EmbeddingProviderType, model: &str) -> usize {
        match provider {
            EmbeddingProviderType::FastEmbed => {
                match model {
                    "all-MiniLM-L6-v2" => 384,
                    "all-MiniLM-L12-v2" => 384,
                    "multilingual-e5-small" => 384,
                    "multilingual-e5-base" => 768,
                    "multilingual-e5-large" => 1024,
                    _ => 384, // Default
                }
            },
            EmbeddingProviderType::Jina => {
                match model {
                    "jina-embeddings-v3" => 1024,
                    "jina-embeddings-v2-base-en" => 768,
                    "jina-embeddings-v2-small-en" => 512,
                    "jina-clip-v1" => 768,
                    _ => 1024, // Default for Jina v3
                }
            },
            EmbeddingProviderType::Voyage => {
                match model {
                    "voyage-3" => 1024,
                    "voyage-3-lite" => 512,
                    "voyage-code-2" => 1536,
                    "voyage-large-2" => 1536,
                    "voyage-law-2" => 1024,
                    "voyage-2" => 1024,
                    _ => 1024, // Default
                }
            },
            EmbeddingProviderType::Google => {
                match model {
                    "text-embedding-004" => 768,
                    "text-embedding-preview-0409" => 768,
                    "text-multilingual-embedding-002" => 768,
                    _ => 768, // Default for Google models
                }
            },
        }
    }

    /// Get default models for each provider
    pub fn get_default_models() -> Self {
        Self {
            provider: EmbeddingProviderType::FastEmbed,
            fastembed: EmbeddingModelConfig {
                code_model: "all-MiniLM-L6-v2".to_string(),
                text_model: "multilingual-e5-small".to_string(),
            },
            jina: EmbeddingModelConfig {
                code_model: "jina-embeddings-v3".to_string(),
                text_model: "jina-embeddings-v3".to_string(),
            },
            voyage: EmbeddingModelConfig {
                code_model: "voyage-code-2".to_string(),
                text_model: "voyage-3".to_string(),
            },
            google: EmbeddingModelConfig {
                code_model: "text-embedding-004".to_string(),
                text_model: "text-embedding-004".to_string(),
            },
        }
    }
}