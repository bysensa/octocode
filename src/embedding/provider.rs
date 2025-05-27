use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

use crate::config::Config;
use super::types::{EmbeddingProviderType, EmbeddingProviderConfig};

/// Trait for embedding providers
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>>;
	async fn generate_embeddings_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>>;
}

/// Create an embedding provider from configuration
pub fn create_embedding_provider(config: &Config) -> Result<Box<dyn EmbeddingProvider>> {
	match config.embedding.provider {
		EmbeddingProviderType::FastEmbed => {
			let model = &config.embedding.fastembed.text_model; // Use text model as default
			Ok(Box::new(FastEmbedProviderImpl::new(model)?))
		}
		EmbeddingProviderType::Jina => {
			let model = &config.embedding.jina.text_model;
			Ok(Box::new(JinaProviderImpl::new(model)))
		}
		EmbeddingProviderType::Voyage => {
			let model = &config.embedding.voyage.text_model;
			Ok(Box::new(VoyageProviderImpl::new(model)))
		}
		EmbeddingProviderType::Google => {
			let model = &config.embedding.google.text_model;
			Ok(Box::new(GoogleProviderImpl::new(model)))
		}
	}
}

/// FastEmbed provider implementation for trait
pub struct FastEmbedProviderImpl {
	model_name: String,
}

impl FastEmbedProviderImpl {
	pub fn new(model: &str) -> Result<Self> {
		Ok(Self {
			model_name: model.to_string(),
		})
	}
}

#[async_trait::async_trait]
impl EmbeddingProvider for FastEmbedProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		FastEmbedProvider::generate_embeddings(text, &self.model_name, false).await
	}

	async fn generate_embeddings_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
		FastEmbedProvider::generate_embeddings_batch(texts, &self.model_name, false).await
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

/// Main embedding provider that delegates to specific implementations
pub struct EmbeddingProviderImpl {
	provider_type: EmbeddingProviderType,
	config: EmbeddingProviderConfig,
}

impl EmbeddingProviderImpl {
	/// Create a provider from configuration
	pub fn from_config(config: &Config) -> Result<Self> {
		let provider_type = config.embedding.provider.clone();
		let provider_config = config.embedding.clone();

		Ok(Self {
			provider_type,
			config: provider_config,
		})
	}

	/// Generate single embedding
	pub async fn generate_embeddings(&self, contents: &str, is_code: bool) -> Result<Vec<f32>> {
		let model = self.config.get_model(&self.provider_type, is_code);

		match self.provider_type {
			EmbeddingProviderType::FastEmbed => {
				FastEmbedProvider::generate_embeddings(contents, model, is_code).await
			},
			EmbeddingProviderType::Jina => {
				JinaProvider::generate_embeddings(contents, model).await
			},
			EmbeddingProviderType::Voyage => {
				VoyageProvider::generate_embeddings(contents, model).await
			},
			EmbeddingProviderType::Google => {
				GoogleProvider::generate_embeddings(contents, model).await
			},
		}
	}

	/// Generate batch embeddings
	pub async fn generate_embeddings_batch(&self, texts: Vec<String>, is_code: bool) -> Result<Vec<Vec<f32>>> {
		let model = self.config.get_model(&self.provider_type, is_code);

		match self.provider_type {
			EmbeddingProviderType::FastEmbed => {
				FastEmbedProvider::generate_embeddings_batch(texts, model, is_code).await
			},
			EmbeddingProviderType::Jina => {
				JinaProvider::generate_embeddings_batch(texts, model).await
			},
			EmbeddingProviderType::Voyage => {
				VoyageProvider::generate_embeddings_batch(texts, model).await
			},
			EmbeddingProviderType::Google => {
				GoogleProvider::generate_embeddings_batch(texts, model).await
			},
		}
	}

	/// Get the vector dimension for current provider configuration
	pub fn get_vector_dimension(&self, is_code: bool) -> usize {
		let model = self.config.get_model(&self.provider_type, is_code);
		self.config.get_vector_dimension(&self.provider_type, model)
	}
}

/// FastEmbed provider implementation
pub struct FastEmbedProvider;

// Create a lazy-loaded FastEmbed embedding model cache
lazy_static::lazy_static! {
	static ref CODE_EMBEDDING_MODEL: Arc<Mutex<Option<Arc<TextEmbedding>>>> = Arc::new(Mutex::new(None));
	static ref TEXT_EMBEDDING_MODEL: Arc<Mutex<Option<Arc<TextEmbedding>>>> = Arc::new(Mutex::new(None));
}

impl FastEmbedProvider {
	/// Initialize models on first use
	fn get_code_embedding_model(model_name: EmbeddingModel) -> Result<Arc<TextEmbedding>> {
		let mut model_guard = CODE_EMBEDDING_MODEL.lock().unwrap();

		if model_guard.is_none() {
			let cache_dir = std::path::PathBuf::from(".octocode/fastembed");
			std::fs::create_dir_all(&cache_dir).context("Failed to create FastEmbed cache directory")?;

			let model = TextEmbedding::try_new(
				InitOptions::new(model_name)
					.with_show_download_progress(true)
					.with_cache_dir(cache_dir),
			)
				.context("Failed to initialize FastEmbed code model")?;

			*model_guard = Some(Arc::new(model));
		}

		Ok(model_guard.as_ref().unwrap().clone())
	}

	fn get_text_embedding_model(model_name: EmbeddingModel) -> Result<Arc<TextEmbedding>> {
		let mut model_guard = TEXT_EMBEDDING_MODEL.lock().unwrap();

		if model_guard.is_none() {
			let cache_dir = std::path::PathBuf::from(".octocode/fastembed");
			std::fs::create_dir_all(&cache_dir).context("Failed to create FastEmbed cache directory")?;

			let model = TextEmbedding::try_new(
				InitOptions::new(model_name)
					.with_show_download_progress(true)
					.with_cache_dir(cache_dir),
			)
				.context("Failed to initialize FastEmbed text model")?;

			*model_guard = Some(Arc::new(model));
		}

		Ok(model_guard.as_ref().unwrap().clone())
	}

	/// Map model name to FastEmbed model enum
	fn map_model_to_fastembed(model: &str) -> EmbeddingModel {
		match model {
			"all-MiniLM-L6-v2" => EmbeddingModel::AllMiniLML6V2,
			"all-MiniLM-L12-v2" => EmbeddingModel::AllMiniLML12V2,
			"multilingual-e5-small" => EmbeddingModel::MultilingualE5Small,
			"multilingual-e5-base" => EmbeddingModel::MultilingualE5Base,
			"multilingual-e5-large" => EmbeddingModel::MultilingualE5Large,
			_ => panic!("Unsupported embedding model: {}", model),
		}
	}

	pub async fn generate_embeddings(contents: &str, model: &str, is_code: bool) -> Result<Vec<f32>> {
		let contents = contents.to_string();
		let model_name = Self::map_model_to_fastembed(model);

		let embedding = tokio::task::spawn_blocking(move || -> Result<Vec<f32>> {
			let model = if is_code {
				Self::get_code_embedding_model(model_name)?
			} else {
				Self::get_text_embedding_model(model_name)?
			};

			let embedding = model.embed(vec![contents], None)?;

			if embedding.is_empty() {
				return Err(anyhow::anyhow!("No embeddings were generated"));
			}

			Ok(embedding[0].clone())
		}).await??;

		Ok(embedding)
	}

	pub async fn generate_embeddings_batch(texts: Vec<String>, model: &str, is_code: bool) -> Result<Vec<Vec<f32>>> {
		let model_name = Self::map_model_to_fastembed(model);

		let embeddings = tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>> {
			let model = if is_code {
				Self::get_code_embedding_model(model_name)?
			} else {
				Self::get_text_embedding_model(model_name)?
			};

			let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
			let embeddings = model.embed(text_refs, None)?;

			Ok(embeddings)
		}).await??;

		Ok(embeddings)
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
