use serde::{Deserialize, Serialize};

/// Supported embedding providers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProviderType {
	FastEmbed,
	Jina,
	Voyage,
	Google,
	SentenceTransformer,
}

impl Default for EmbeddingProviderType {
	fn default() -> Self {
		Self::FastEmbed
	}
}

/// Configuration for embedding models (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
	/// Code embedding model (format: "provider:model")
	#[serde(default = "default_code_model")]
	pub code_model: String,

	/// Text embedding model (format: "provider:model")
	#[serde(default = "default_text_model")]
	pub text_model: String,

	/// Jina AI configuration (API key only)
	#[serde(default)]
	pub jina: JinaConfig,

	/// Voyage AI configuration (API key only)
	#[serde(default)]
	pub voyage: VoyageConfig,

	/// Google configuration (API key only)
	#[serde(default)]
	pub google: GoogleConfig,
}

/// Default code model
fn default_code_model() -> String {
	"fastembed:all-MiniLM-L6-v2".to_string()
}

/// Default text model
fn default_text_model() -> String {
	"fastembed:multilingual-e5-small".to_string()
}

/// Jina AI specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JinaConfig {
	pub api_key: Option<String>,
}

/// Voyage AI specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoyageConfig {
	pub api_key: Option<String>,
}

/// Google specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoogleConfig {
	pub api_key: Option<String>,
}

/// Parse provider and model from a string in format "provider:model"
pub fn parse_provider_model(input: &str) -> (EmbeddingProviderType, String) {
	if let Some((provider_str, model)) = input.split_once(':') {
		let provider = match provider_str.to_lowercase().as_str() {
			"fastembed" => EmbeddingProviderType::FastEmbed,
			"jinaai" | "jina" => EmbeddingProviderType::Jina,
			"voyageai" | "voyage" => EmbeddingProviderType::Voyage,
			"google" => EmbeddingProviderType::Google,
			"sentencetransformer" | "st" | "huggingface" | "hf" => EmbeddingProviderType::SentenceTransformer,
			_ => EmbeddingProviderType::FastEmbed, // Default fallback
		};
		(provider, model.to_string())
	} else {
		// Legacy format - assume FastEmbed for backward compatibility
		(EmbeddingProviderType::FastEmbed, input.to_string())
	}
}

impl EmbeddingConfig {
	/// Get the currently active provider based on the code model
	pub fn get_active_provider(&self) -> EmbeddingProviderType {
		let (provider, _) = parse_provider_model(&self.code_model);
		provider
	}

	/// Get API key for a specific provider (checks environment variables first)
	pub fn get_api_key(&self, provider: &EmbeddingProviderType) -> Option<String> {
		match provider {
			EmbeddingProviderType::Jina => {
				// Environment variable takes priority
				std::env::var("JINA_API_KEY").ok()
					.or_else(|| self.jina.api_key.clone())
			},
			EmbeddingProviderType::Voyage => {
				std::env::var("VOYAGE_API_KEY").ok()
					.or_else(|| self.voyage.api_key.clone())
			},
			EmbeddingProviderType::Google => {
				std::env::var("GOOGLE_API_KEY").ok()
					.or_else(|| self.google.api_key.clone())
			},
			_ => None, // FastEmbed and SentenceTransformer don't need API keys
		}
	}

	/// Get the vector dimension for a specific provider and model
	pub fn get_vector_dimension(&self, provider: &EmbeddingProviderType, model: &str) -> usize {
		match provider {
			EmbeddingProviderType::FastEmbed => {
				match model {
					"sentence-transformers/all-MiniLM-L6-v2" => 384,
					"sentence-transformers/all-MiniLM-L6-v2-quantized" => 384,
					"sentence-transformers/all-MiniLM-L12-v2" => 768,
					"sentence-transformers/all-MiniLM-L12-v2-quantized" => 768,
					"BAAI/bge-base-en-v1.5" => 768,
					"BAAI/bge-base-en-v1.5-quantized" => 768,
					"BAAI/bge-large-en-v1.5" => 1024,
					"BAAI/bge-large-en-v1.5-quantized" => 1024,
					"BAAI/bge-small-en-v1.5" => 384,
					"BAAI/bge-small-en-v1.5-quantized" => 384,
					"nomic-ai/nomic-embed-text-v1" => 768,
					"nomic-ai/nomic-embed-text-v1.5" => 768,
					"nomic-ai/nomic-embed-text-v1.5-quantized" => 768,
					"sentence-transformers/paraphrase-MiniLM-L6-v2" => 384,
					"sentence-transformers/paraphrase-MiniLM-L6-v2-quantized" => 384,
					"sentence-transformers/paraphrase-mpnet-base-v2" => 768,
					"BAAI/bge-small-zh-v1.5" => 512,
					"BAAI/bge-large-zh-v1.5" => 1024,
					"lightonai/modernbert-embed-large" => 1024,
					"intfloat/multilingual-e5-small" | "multilingual-e5-small" => 384,
					"intfloat/multilingual-e5-base" | "multilingual-e5-base" => 768,
					"intfloat/multilingual-e5-large" | "multilingual-e5-large" => 1024,
					"mixedbread-ai/mxbai-embed-large-v1" => 1024,
					"mixedbread-ai/mxbai-embed-large-v1-quantized" => 1024,
					"Alibaba-NLP/gte-base-en-v1.5" => 768,
					"Alibaba-NLP/gte-base-en-v1.5-quantized" => 768,
					"Alibaba-NLP/gte-large-en-v1.5" => 1024,
					"Alibaba-NLP/gte-large-en-v1.5-quantized" => 1024,
					"Qdrant/clip-ViT-B-32-text" => 512,
					"jinaai/jina-embeddings-v2-base-code" => 768,
					_ => panic!("Unsupported embedding model: {}", model),
				}
			},
			EmbeddingProviderType::Jina => {
				match model {
					"jina-embeddings-v3" => 1024,
					"jina-embeddings-v2-base-en" => 768,
					"jina-embeddings-v2-base-code" => 768,
					"jina-embeddings-v2-small-en" => 512,
					"jina-clip-v1" => 768,
					_ => panic!("Unsupported embedding model: {}", model),
				}
			},
			EmbeddingProviderType::Voyage => {
				match model {
					"voyage-3.5" => 1024,
					"voyage-3.5-lite" => 1024,
					"voyage-3-large" => 1024,
					"voyage-code-2" => 1536,
					"voyage-code-3" => 1024,
					"voyage-finance-2" => 1024,
					"voyage-law-2" => 1024,
					"voyage-2" => 1024,
					_ => panic!("Unsupported embedding model: {}", model),
				}
			},
			EmbeddingProviderType::Google => {
				match model {
					"text-embedding-004" => 768,
					"text-embedding-preview-0409" => 768,
					"text-multilingual-embedding-002" => 768,
					_ => panic!("Unsupported embedding model: {}", model),
				}
			},
			EmbeddingProviderType::SentenceTransformer => {
				// Common SentenceTransformer model dimensions
				match model {
					"sentence-transformers/all-MiniLM-L6-v2" => 384,
					"sentence-transformers/all-MiniLM-L12-v2" => 384,
					"sentence-transformers/all-mpnet-base-v2" => 768,
					"sentence-transformers/all-roberta-large-v1" => 1024,
					"sentence-transformers/paraphrase-MiniLM-L6-v2" => 384,
					"sentence-transformers/paraphrase-mpnet-base-v2" => 768,
					"microsoft/codebert-base" => 768,
					"microsoft/unixcoder-base" => 768,
					"sentence-transformers/multi-qa-mpnet-base-dot-v1" => 768,
					"BAAI/bge-small-en-v1.5" => 384,
					"BAAI/bge-base-en-v1.5" => 768,
					"BAAI/bge-large-en-v1.5" => 1024,
					_ => panic!("Unsupported embedding model: {}", model),
				}
			},
		}
	}
}
