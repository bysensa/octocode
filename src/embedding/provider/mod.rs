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

//! Embedding providers module
//!
//! This module contains implementations for different embedding providers.
//! Each provider can be optionally compiled based on cargo features.

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::LazyLock;
use std::time::Duration;

use super::types::{EmbeddingProviderType, InputType};

// Shared HTTP client with connection pooling for optimal performance
static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
	Client::builder()
		.pool_max_idle_per_host(10)
		.pool_idle_timeout(Duration::from_secs(30))
		.timeout(Duration::from_secs(120)) // Increased from 60s to 120s for embedding APIs
		.connect_timeout(Duration::from_secs(10))
		.build()
		.expect("Failed to create HTTP client")
});

// Feature-specific provider modules
#[cfg(feature = "fastembed")]
pub mod fastembed;
#[cfg(feature = "huggingface")]
pub mod huggingface;

// Re-export providers
#[cfg(feature = "fastembed")]
pub use fastembed::{FastEmbedProvider, FastEmbedProviderImpl};
#[cfg(feature = "huggingface")]
pub use huggingface::HuggingFaceProvider;

/// Trait for embedding providers
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>>;
	async fn generate_embeddings_batch(
		&self,
		texts: Vec<String>,
		input_type: InputType,
	) -> Result<Vec<Vec<f32>>>;

	/// Get the vector dimension for this provider's model
	fn get_dimension(&self) -> usize;

	/// Validate if the model is supported (optional, defaults to true)
	fn is_model_supported(&self) -> bool {
		true
	}
}

/// Create an embedding provider from provider type and model
pub fn create_embedding_provider_from_parts(
	provider: &EmbeddingProviderType,
	model: &str,
) -> Result<Box<dyn EmbeddingProvider>> {
	match provider {
		EmbeddingProviderType::FastEmbed => {
			#[cfg(feature = "fastembed")]
			{
				Ok(Box::new(FastEmbedProviderImpl::new(model)?))
			}
			#[cfg(not(feature = "fastembed"))]
			{
				Err(anyhow::anyhow!("FastEmbed support is not compiled in. Please rebuild with --features fastembed"))
			}
		}
		EmbeddingProviderType::Jina => Ok(Box::new(JinaProviderImpl::new(model)?)),
		EmbeddingProviderType::Voyage => Ok(Box::new(VoyageProviderImpl::new(model)?)),
		EmbeddingProviderType::Google => Ok(Box::new(GoogleProviderImpl::new(model))),
		EmbeddingProviderType::HuggingFace => {
			#[cfg(feature = "huggingface")]
			{
				Ok(Box::new(HuggingFaceProviderImpl::new(model)?))
			}
			#[cfg(not(feature = "huggingface"))]
			{
				Err(anyhow::anyhow!("HuggingFace support is not compiled in. Please rebuild with --features huggingface"))
			}
		}
	}
}

/// HuggingFace provider implementation for trait
#[cfg(feature = "huggingface")]
pub struct HuggingFaceProviderImpl {
	model_name: String,
	dimension: usize,
}

#[cfg(feature = "huggingface")]
impl HuggingFaceProviderImpl {
	pub fn new(model: &str) -> Result<Self> {
		#[cfg(not(feature = "huggingface"))]
		{
			Err(anyhow::anyhow!("HuggingFace provider requires 'huggingface' feature to be enabled. Cannot validate model '{}' without Hub API access.", model))
		}

		#[cfg(feature = "huggingface")]
		{
			let dimension = Self::get_model_dimension_static(model)?;
			Ok(Self {
				model_name: model.to_string(),
				dimension,
			})
		}
	}

	#[cfg(feature = "huggingface")]
	fn get_model_dimension_static(model: &str) -> Result<usize> {
		// DYNAMIC discovery only - get dimension from HuggingFace Hub config.json
		if let Some(dimension) = Self::get_dimension_from_hub_config(model) {
			return Ok(dimension);
		}

		// NO STATIC FALLBACKS - if we can't get dimension dynamically, fail properly
		Err(anyhow::anyhow!("Failed to get dimension for HuggingFace model '{}' from Hub API. Cannot proceed without dynamic model discovery.", model))
	}

	/// Get model dimension from HuggingFace Hub config.json
	#[cfg(feature = "huggingface")]
	fn get_dimension_from_hub_config(model_name: &str) -> Option<usize> {
		// Use blocking runtime since this is called from sync context
		if let Ok(rt) = tokio::runtime::Runtime::new() {
			return rt.block_on(Self::fetch_dimension_from_hub(model_name));
		}

		None
	}

	/// Async method to fetch dimension from HuggingFace Hub config.json
	#[cfg(feature = "huggingface")]
	async fn fetch_dimension_from_hub(model_name: &str) -> Option<usize> {
		use hf_hub::{api::tokio::Api, Repo, RepoType};
		use serde_json::Value;

		// Try to fetch config.json from HuggingFace Hub
		let api = Api::new().ok()?;
		let repo = api.repo(Repo::new(model_name.to_string(), RepoType::Model));

		// Try to get config.json
		let config_content = match repo.get("config.json").await {
			Ok(content) => content,
			Err(e) => {
				tracing::debug!("Failed to fetch config.json for {}: {}", model_name, e);
				return None;
			}
		};

		// Parse JSON and extract dimension
		let config_text = match std::fs::read_to_string(&config_content) {
			Ok(text) => text,
			Err(e) => {
				tracing::debug!("Failed to read config.json for {}: {}", model_name, e);
				return None;
			}
		};

		let config: Value = match serde_json::from_str(&config_text) {
			Ok(config) => config,
			Err(e) => {
				tracing::debug!("Failed to parse config.json for {}: {}", model_name, e);
				return None;
			}
		};

		// Try different field names that contain embedding dimensions
		let dimension_fields = ["hidden_size", "d_model", "embedding_size", "dim"];

		for field in &dimension_fields {
			if let Some(dim) = config.get(field).and_then(|v| v.as_u64()) {
				tracing::info!(
					"Found dimension {} for model {} from config.json field '{}'",
					dim,
					model_name,
					field
				);
				return Some(dim as usize);
			}
		}

		tracing::debug!("No dimension field found in config.json for {}", model_name);
		None
	}
}

#[cfg(feature = "huggingface")]
#[async_trait::async_trait]
impl EmbeddingProvider for HuggingFaceProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		HuggingFaceProvider::generate_embeddings(text, &self.model_name).await
	}

	async fn generate_embeddings_batch(
		&self,
		texts: Vec<String>,
		input_type: InputType,
	) -> Result<Vec<Vec<f32>>> {
		// Apply prefix manually for HuggingFace (doesn't support input_type API)
		let processed_texts: Vec<String> = texts
			.into_iter()
			.map(|text| input_type.apply_prefix(&text))
			.collect();
		HuggingFaceProvider::generate_embeddings_batch(processed_texts, &self.model_name).await
	}

	fn get_dimension(&self) -> usize {
		self.dimension
	}

	fn is_model_supported(&self) -> bool {
		// For HuggingFace, we support many models, so return true for most cases
		// The actual validation happens when trying to load the model
		true
	}
}

/// Jina provider implementation for trait
pub struct JinaProviderImpl {
	model_name: String,
	dimension: usize,
}

impl JinaProviderImpl {
	pub fn new(model: &str) -> Result<Self> {
		// Validate model first - fail fast if unsupported
		let supported_models = [
			"jina-embeddings-v3",
			"jina-embeddings-v2-base-en",
			"jina-embeddings-v2-base-code",
			"jina-embeddings-v2-small-en",
			"jina-clip-v1",
		];

		if !supported_models.contains(&model) {
			return Err(anyhow::anyhow!(
				"Unsupported Jina model: '{}'. Supported models: {:?}",
				model,
				supported_models
			));
		}

		let dimension = Self::get_model_dimension_static(model);
		Ok(Self {
			model_name: model.to_string(),
			dimension,
		})
	}

	fn get_model_dimension_static(model: &str) -> usize {
		match model {
			"jina-embeddings-v3" => 1024,
			"jina-embeddings-v2-base-en" => 768,
			"jina-embeddings-v2-base-code" => 768,
			"jina-embeddings-v2-small-en" => 512,
			"jina-clip-v1" => 768,
			_ => {
				// This should never be reached due to validation in new()
				panic!(
					"Invalid Jina model '{}' passed to get_model_dimension_static",
					model
				);
			}
		}
	}
}

#[async_trait::async_trait]
impl EmbeddingProvider for JinaProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		JinaProvider::generate_embeddings(text, &self.model_name).await
	}

	async fn generate_embeddings_batch(
		&self,
		texts: Vec<String>,
		input_type: InputType,
	) -> Result<Vec<Vec<f32>>> {
		// Apply prefix manually for Jina (doesn't support input_type API)
		let processed_texts: Vec<String> = texts
			.into_iter()
			.map(|text| input_type.apply_prefix(&text))
			.collect();
		JinaProvider::generate_embeddings_batch(processed_texts, &self.model_name).await
	}

	fn get_dimension(&self) -> usize {
		self.dimension
	}

	fn is_model_supported(&self) -> bool {
		// REAL validation - only support actual Jina models
		matches!(
			self.model_name.as_str(),
			"jina-embeddings-v3"
				| "jina-embeddings-v2-base-en"
				| "jina-embeddings-v2-base-code"
				| "jina-embeddings-v2-small-en"
				| "jina-clip-v1"
		)
	}
}

/// Voyage provider implementation for trait
pub struct VoyageProviderImpl {
	model_name: String,
	dimension: usize,
}

impl VoyageProviderImpl {
	pub fn new(model: &str) -> Result<Self> {
		// Validate model first - fail fast if unsupported
		let supported_models = [
			"voyage-3.5",
			"voyage-3.5-lite",
			"voyage-3-large",
			"voyage-code-2",
			"voyage-code-3",
			"voyage-finance-2",
			"voyage-law-2",
			"voyage-2",
		];

		if !supported_models.contains(&model) {
			return Err(anyhow::anyhow!(
				"Unsupported Voyage model: '{}'. Supported models: {:?}",
				model,
				supported_models
			));
		}

		let dimension = Self::get_model_dimension_static(model);
		Ok(Self {
			model_name: model.to_string(),
			dimension,
		})
	}

	fn get_model_dimension_static(model: &str) -> usize {
		match model {
			"voyage-3.5" => 1024,
			"voyage-3.5-lite" => 1024,
			"voyage-3-large" => 1024,
			"voyage-code-2" => 1536,
			"voyage-code-3" => 1024,
			"voyage-finance-2" => 1024,
			"voyage-law-2" => 1024,
			"voyage-2" => 1024,
			_ => {
				// This should never be reached due to validation in new()
				panic!(
					"Invalid Voyage model '{}' passed to get_model_dimension_static",
					model
				);
			}
		}
	}
}

#[async_trait::async_trait]
impl EmbeddingProvider for VoyageProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		VoyageProvider::generate_embeddings(text, &self.model_name).await
	}

	async fn generate_embeddings_batch(
		&self,
		texts: Vec<String>,
		input_type: InputType,
	) -> Result<Vec<Vec<f32>>> {
		VoyageProvider::generate_embeddings_batch(texts, &self.model_name, input_type).await
	}

	fn get_dimension(&self) -> usize {
		self.dimension
	}

	fn is_model_supported(&self) -> bool {
		// REAL validation - only support actual Voyage models, NO HALLUCINATIONS
		matches!(
			self.model_name.as_str(),
			"voyage-3.5"
				| "voyage-3.5-lite"
				| "voyage-3-large"
				| "voyage-code-2"
				| "voyage-code-3"
				| "voyage-finance-2"
				| "voyage-law-2"
				| "voyage-2"
		)
	}
}

/// Google provider implementation for trait
pub struct GoogleProviderImpl {
	model_name: String,
	dimension: usize,
}

impl GoogleProviderImpl {
	pub fn new(model: &str) -> Self {
		let dimension = Self::get_model_dimension_static(model);
		Self {
			model_name: model.to_string(),
			dimension,
		}
	}

	fn get_model_dimension_static(model: &str) -> usize {
		// DYNAMIC discovery only - NO STATIC FALLBACKS
		// For API providers, we should query their API for model info
		// For now, panic to force proper dynamic implementation
		panic!("Google provider must implement dynamic model discovery for '{}'. No static fallbacks allowed.", model);
	}
}

#[async_trait::async_trait]
impl EmbeddingProvider for GoogleProviderImpl {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		GoogleProvider::generate_embeddings(text, &self.model_name).await
	}

	async fn generate_embeddings_batch(
		&self,
		texts: Vec<String>,
		input_type: InputType,
	) -> Result<Vec<Vec<f32>>> {
		// Apply prefix manually for Google (doesn't support input_type API)
		let processed_texts: Vec<String> = texts
			.into_iter()
			.map(|text| input_type.apply_prefix(&text))
			.collect();
		GoogleProvider::generate_embeddings_batch(processed_texts, &self.model_name).await
	}

	fn get_dimension(&self) -> usize {
		self.dimension
	}

	fn is_model_supported(&self) -> bool {
		matches!(
			self.model_name.as_str(),
			"text-embedding-004"
				| "text-embedding-preview-0409"
				| "text-multilingual-embedding-002"
		)
	}
}

/// Jina provider implementation
pub struct JinaProvider;

impl JinaProvider {
	pub async fn generate_embeddings(contents: &str, model: &str) -> Result<Vec<f32>> {
		let result = Self::generate_embeddings_batch(vec![contents.to_string()], model).await?;
		result
			.first()
			.cloned()
			.ok_or_else(|| anyhow::anyhow!("No embeddings found"))
	}

	pub async fn generate_embeddings_batch(
		texts: Vec<String>,
		model: &str,
	) -> Result<Vec<Vec<f32>>> {
		let jina_api_key =
			std::env::var("JINA_API_KEY").context("JINA_API_KEY environment variable not set")?;

		let response = HTTP_CLIENT
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
		let result =
			Self::generate_embeddings_batch(vec![contents.to_string()], model, InputType::None)
				.await?;
		result
			.first()
			.cloned()
			.ok_or_else(|| anyhow::anyhow!("No embeddings found"))
	}

	pub async fn generate_embeddings_batch(
		texts: Vec<String>,
		model: &str,
		input_type: InputType,
	) -> Result<Vec<Vec<f32>>> {
		let voyage_api_key = std::env::var("VOYAGE_API_KEY")
			.context("VOYAGE_API_KEY environment variable not set")?;

		// Build request body with optional input_type
		let mut request_body = json!({
			"input": texts,
			"model": model,
		});

		// Add input_type if specified (Voyage API native support)
		if let Some(input_type_str) = input_type.as_api_str() {
			request_body["input_type"] = json!(input_type_str);
		}

		let response = HTTP_CLIENT
			.post("https://api.voyageai.com/v1/embeddings")
			.header("Authorization", format!("Bearer {}", voyage_api_key))
			.header("Content-Type", "application/json")
			.json(&request_body)
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
		result
			.first()
			.cloned()
			.ok_or_else(|| anyhow::anyhow!("No embeddings found"))
	}

	pub async fn generate_embeddings_batch(
		texts: Vec<String>,
		model: &str,
	) -> Result<Vec<Vec<f32>>> {
		let google_api_key = std::env::var("GOOGLE_API_KEY")
			.context("GOOGLE_API_KEY environment variable not set")?;

		// For batch processing, we'll need to send individual requests as Google's API structure is different
		let mut all_embeddings = Vec::new();

		for text in texts {
			let response = HTTP_CLIENT
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
