use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

use super::types::EmbeddingProviderType;

pub mod sentence_transformer;
use sentence_transformer::SentenceTransformerProvider;

/// Trait for embedding providers
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>>;
	async fn generate_embeddings_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>>;
}

/// Create an embedding provider from provider type and model
pub fn create_embedding_provider_from_parts(provider: &EmbeddingProviderType, model: &str) -> Result<Box<dyn EmbeddingProvider>> {
	match provider {
		EmbeddingProviderType::FastEmbed => {
			Ok(Box::new(FastEmbedProviderImpl::new(model)?))
		}
		EmbeddingProviderType::Jina => {
			Ok(Box::new(JinaProviderImpl::new(model)))
		}
		EmbeddingProviderType::Voyage => {
			Ok(Box::new(VoyageProviderImpl::new(model)))
		}
		EmbeddingProviderType::Google => {
			Ok(Box::new(GoogleProviderImpl::new(model)))
		}
		EmbeddingProviderType::SentenceTransformer => {
			Ok(Box::new(SentenceTransformerProviderImpl::new(model)))
		}
	}
}



/// FastEmbed provider implementation for trait
pub struct FastEmbedProviderImpl {
	model: Arc<TextEmbedding>,
}

impl FastEmbedProviderImpl {
	pub fn new(model_name: &str) -> Result<Self> {
		let model_enum = FastEmbedProvider::map_model_to_fastembed(model_name);
		
		// Use system-wide cache for FastEmbed models
		let cache_dir = crate::storage::get_fastembed_cache_dir()
			.context("Failed to get FastEmbed cache directory")?;

		let model = TextEmbedding::try_new(
			InitOptions::new(model_enum)
				.with_show_download_progress(true)
				.with_cache_dir(cache_dir),
		)
			.context("Failed to initialize FastEmbed model")?;

		Ok(Self {
			model: Arc::new(model),
		})
	}
}

#[async_trait::async_trait]
impl EmbeddingProvider for FastEmbedProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		let text = text.to_string();
		let model = self.model.clone();

		let embedding = tokio::task::spawn_blocking(move || -> Result<Vec<f32>> {
			let embedding = model.embed(vec![text], None)?;

			if embedding.is_empty() {
				return Err(anyhow::anyhow!("No embeddings were generated"));
			}

			Ok(embedding[0].clone())
		}).await??;

		Ok(embedding)
	}

	async fn generate_embeddings_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
		let model = self.model.clone();

		let embeddings = tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>> {
			let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
			let embeddings = model.embed(text_refs, None)?;

			Ok(embeddings)
		}).await??;

		Ok(embeddings)
	}
}

/// Jina provider implementation for trait
pub struct JinaProviderImpl {
	model_name: String,
}

impl JinaProviderImpl {
	pub fn new(model: &str) -> Self {
		Self {
			model_name: model.to_string(),
		}
	}
}

#[async_trait::async_trait]
impl EmbeddingProvider for JinaProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		JinaProvider::generate_embeddings(text, &self.model_name).await
	}

	async fn generate_embeddings_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
		JinaProvider::generate_embeddings_batch(texts, &self.model_name).await
	}
}

/// Voyage provider implementation for trait
pub struct VoyageProviderImpl {
	model_name: String,
}

impl VoyageProviderImpl {
	pub fn new(model: &str) -> Self {
		Self {
			model_name: model.to_string(),
		}
	}
}

#[async_trait::async_trait]
impl EmbeddingProvider for VoyageProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		VoyageProvider::generate_embeddings(text, &self.model_name).await
	}

	async fn generate_embeddings_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
		VoyageProvider::generate_embeddings_batch(texts, &self.model_name).await
	}
}

/// Google provider implementation for trait
pub struct GoogleProviderImpl {
	model_name: String,
}

impl GoogleProviderImpl {
	pub fn new(model: &str) -> Self {
		Self {
			model_name: model.to_string(),
		}
	}
}

#[async_trait::async_trait]
impl EmbeddingProvider for GoogleProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		GoogleProvider::generate_embeddings(text, &self.model_name).await
	}

	async fn generate_embeddings_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
		GoogleProvider::generate_embeddings_batch(texts, &self.model_name).await
	}
}

/// SentenceTransformer provider implementation for trait
pub struct SentenceTransformerProviderImpl {
	model_name: String,
}

impl SentenceTransformerProviderImpl {
	pub fn new(model: &str) -> Self {
		Self {
			model_name: model.to_string(),
		}
	}
}

#[async_trait::async_trait]
impl EmbeddingProvider for SentenceTransformerProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		SentenceTransformerProvider::generate_embeddings(text, &self.model_name).await
	}

	async fn generate_embeddings_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
		SentenceTransformerProvider::generate_embeddings_batch(texts, &self.model_name).await
	}
}



/// FastEmbed provider implementation
pub struct FastEmbedProvider;

impl FastEmbedProvider {
	/// Map model name to FastEmbed model enum
	fn map_model_to_fastembed(model: &str) -> EmbeddingModel {
		match model {
			"sentence-transformers/all-MiniLM-L6-v2" => EmbeddingModel::AllMiniLML6V2,
			"sentence-transformers/all-MiniLM-L6-v2-quantized" => EmbeddingModel::AllMiniLML6V2Q,
			"sentence-transformers/all-MiniLM-L12-v2" | "all-MiniLM-L12-v2" => EmbeddingModel::AllMiniLML12V2,
			"sentence-transformers/all-MiniLM-L12-v2-quantized" => EmbeddingModel::AllMiniLML12V2Q,
			"BAAI/bge-base-en-v1.5" => EmbeddingModel::BGEBaseENV15,
			"BAAI/bge-base-en-v1.5-quantized" => EmbeddingModel::BGEBaseENV15Q,
			"BAAI/bge-large-en-v1.5" => EmbeddingModel::BGELargeENV15,
			"BAAI/bge-large-en-v1.5-quantized" => EmbeddingModel::BGELargeENV15Q,
			"BAAI/bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
			"BAAI/bge-small-en-v1.5-quantized" => EmbeddingModel::BGESmallENV15Q,
			"nomic-ai/nomic-embed-text-v1" => EmbeddingModel::NomicEmbedTextV1,
			"nomic-ai/nomic-embed-text-v1.5" => EmbeddingModel::NomicEmbedTextV15,
			"nomic-ai/nomic-embed-text-v1.5-quantized" => EmbeddingModel::NomicEmbedTextV15Q,
			"sentence-transformers/paraphrase-MiniLM-L6-v2" => EmbeddingModel::ParaphraseMLMiniLML12V2,
			"sentence-transformers/paraphrase-MiniLM-L6-v2-quantized" => EmbeddingModel::ParaphraseMLMiniLML12V2Q,
			"sentence-transformers/paraphrase-mpnet-base-v2" => EmbeddingModel::ParaphraseMLMpnetBaseV2,
			"BAAI/bge-small-zh-v1.5" => EmbeddingModel::BGESmallZHV15,
			"BAAI/bge-large-zh-v1.5" => EmbeddingModel::BGELargeZHV15,
			"lightonai/modernbert-embed-large" => EmbeddingModel::ModernBertEmbedLarge,
			"intfloat/multilingual-e5-small" | "multilingual-e5-small" => EmbeddingModel::MultilingualE5Small,
			"intfloat/multilingual-e5-base" | "multilingual-e5-base" => EmbeddingModel::MultilingualE5Base,
			"intfloat/multilingual-e5-large" | "multilingual-e5-large" => EmbeddingModel::MultilingualE5Large,
			"mixedbread-ai/mxbai-embed-large-v1" => EmbeddingModel::MxbaiEmbedLargeV1,
			"mixedbread-ai/mxbai-embed-large-v1-quantized" => EmbeddingModel::MxbaiEmbedLargeV1Q,
			"Alibaba-NLP/gte-base-en-v1.5" => EmbeddingModel::GTEBaseENV15,
			"Alibaba-NLP/gte-base-en-v1.5-quantized" => EmbeddingModel::GTEBaseENV15Q,
			"Alibaba-NLP/gte-large-en-v1.5" => EmbeddingModel::GTELargeENV15,
			"Alibaba-NLP/gte-large-en-v1.5-quantized" => EmbeddingModel::GTELargeENV15Q,
			"Qdrant/clip-ViT-B-32-text" => EmbeddingModel::ClipVitB32,
			"jinaai/jina-embeddings-v2-base-code" => EmbeddingModel::JinaEmbeddingsV2BaseCode,
			_ => panic!("Unsupported embedding model: {}", model),
		}
	}
}

/// Jina provider implementation
pub struct JinaProvider;

impl JinaProvider {
	pub async fn generate_embeddings(contents: &str, model: &str) -> Result<Vec<f32>> {
		let result = Self::generate_embeddings_batch(vec![contents.to_string()], model).await?;
		result.first()
			.cloned()
			.ok_or_else(|| anyhow::anyhow!("No embeddings found"))
	}

	pub async fn generate_embeddings_batch(texts: Vec<String>, model: &str) -> Result<Vec<Vec<f32>>> {
		let client = Client::new();
		let jina_api_key = std::env::var("JINA_API_KEY")
			.context("JINA_API_KEY environment variable not set")?;

		let response = client
			.post("https://api.jina.ai/v1/embeddings")
			.header("Authorization", format!("Bearer {}", jina_api_key))
			.json(&json!({
				"input": texts,
				"model": model,
			}))
			.send()
		.await?;

		let response_json: Value = response.json().await?;

		let embeddings = response_json["data"]
			.as_array()
			.context("Failed to get embeddings array")?
			.iter()
			.map(|data| {
				data["embedding"]
					.as_array()
					.unwrap_or(&Vec::new())
					.iter()
					.map(|v| v.as_f64().unwrap_or_default() as f32)
					.collect()
			})
			.collect();

		Ok(embeddings)
	}
}

/// Voyage AI provider implementation
pub struct VoyageProvider;

impl VoyageProvider {
	pub async fn generate_embeddings(contents: &str, model: &str) -> Result<Vec<f32>> {
		let result = Self::generate_embeddings_batch(vec![contents.to_string()], model).await?;
		result.first()
			.cloned()
			.ok_or_else(|| anyhow::anyhow!("No embeddings found"))
	}

	pub async fn generate_embeddings_batch(texts: Vec<String>, model: &str) -> Result<Vec<Vec<f32>>> {
		let client = Client::new();
		let voyage_api_key = std::env::var("VOYAGE_API_KEY")
			.context("VOYAGE_API_KEY environment variable not set")?;

		let response = client
			.post("https://api.voyageai.com/v1/embeddings")
			.header("Authorization", format!("Bearer {}", voyage_api_key))
			.header("Content-Type", "application/json")
			.json(&json!({
				"input": texts,
				"model": model,
			}))
			.send()
		.await?;

		if !response.status().is_success() {
			let error_text = response.text().await?;
			return Err(anyhow::anyhow!("Voyage API error: {}", error_text));
		}

		let response_json: Value = response.json().await?;

		let embeddings = response_json["data"]
			.as_array()
			.context("Failed to get embeddings array")?
			.iter()
			.map(|data| {
				data["embedding"]
					.as_array()
					.unwrap_or(&Vec::new())
					.iter()
					.map(|v| v.as_f64().unwrap_or_default() as f32)
					.collect()
			})
			.collect();

		Ok(embeddings)
	}
}

/// Google provider implementation
pub struct GoogleProvider;

impl GoogleProvider {
	pub async fn generate_embeddings(contents: &str, model: &str) -> Result<Vec<f32>> {
		let result = Self::generate_embeddings_batch(vec![contents.to_string()], model).await?;
		result.first()
			.cloned()
			.ok_or_else(|| anyhow::anyhow!("No embeddings found"))
	}

	pub async fn generate_embeddings_batch(texts: Vec<String>, model: &str) -> Result<Vec<Vec<f32>>> {
		let client = Client::new();
		let google_api_key = std::env::var("GOOGLE_API_KEY")
			.context("GOOGLE_API_KEY environment variable not set")?;

		// For batch processing, we'll need to send individual requests as Google's API structure is different
		let mut all_embeddings = Vec::new();

		for text in texts {
			let response = client
				.post(format!("https://generativelanguage.googleapis.com/v1beta/models/{}:embedContent?key={}", model, google_api_key))
				.header("Content-Type", "application/json")
				.json(&json!({
					"content": {
						"parts": [{
							"text": text
						}]
					}
				}))
				.send()
			.await?;

			if !response.status().is_success() {
				let error_text = response.text().await?;
				return Err(anyhow::anyhow!("Google API error: {}", error_text));
			}

			let response_json: Value = response.json().await?;

			let embedding = response_json["embedding"]["values"]
				.as_array()
				.context("Failed to get embedding values")?
				.iter()
				.map(|v| v.as_f64().unwrap_or_default() as f32)
				.collect();

			all_embeddings.push(embedding);
		}

		Ok(all_embeddings)
	}
}
