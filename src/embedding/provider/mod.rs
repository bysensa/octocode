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

use super::types::EmbeddingProviderType;

// Feature-specific provider modules
pub mod fastembed;
pub mod sentence_transformer;

// Re-export providers
pub use fastembed::{FastEmbedProvider, FastEmbedProviderImpl};
pub use sentence_transformer::SentenceTransformerProvider;

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
			#[cfg(feature = "fastembed")]
			{
				Ok(Box::new(FastEmbedProviderImpl::new(model)?))
			}
			#[cfg(not(feature = "fastembed"))]
			{
				Err(anyhow::anyhow!("FastEmbed support is not compiled in. Please rebuild with --features fastembed"))
			}
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
			#[cfg(feature = "sentence-transformer")]
			{
				Ok(Box::new(SentenceTransformerProviderImpl::new(model)))
			}
			#[cfg(not(feature = "sentence-transformer"))]
			{
				Err(anyhow::anyhow!("SentenceTransformer support is not compiled in. Please rebuild with --features sentence-transformer"))
			}
		}
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