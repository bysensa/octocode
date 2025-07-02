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

/*!
 * HuggingFace Provider Implementation
 *
 * This module provides local embedding generation using HuggingFace models via the Candle library.
 * It supports multiple model architectures with safetensors format from the HuggingFace Hub.
 *
 * Key features:
 * - Automatic model downloading and caching
 * - Local CPU-based inference (GPU support can be added)
 * - Thread-safe model cache for efficient reuse
 * - Mean pooling and L2 normalization for sentence embeddings
 * - Full compatibility with provider:model syntax
 * - Dynamic model architecture detection
 *
 * Usage:
 * - Set provider: `octocode config --embedding-provider huggingface`
 * - Set models: `octocode config --code-embedding-model "huggingface:jinaai/jina-embeddings-v2-base-code"`
 * - Popular models: jinaai/jina-embeddings-v2-base-code, sentence-transformers/all-mpnet-base-v2
 *
 * Models are automatically downloaded to the system cache directory and reused across sessions.
 */

// When huggingface feature is enabled
#[cfg(feature = "huggingface")]
use anyhow::{Context, Result};
#[cfg(feature = "huggingface")]
use candle_core::{DType, Device, Tensor};
#[cfg(feature = "huggingface")]
use candle_nn::VarBuilder;
#[cfg(feature = "huggingface")]
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
#[cfg(feature = "huggingface")]
use hf_hub::{api::tokio::Api, Repo, RepoType};
#[cfg(feature = "huggingface")]
use std::collections::HashMap;
#[cfg(feature = "huggingface")]
use std::sync::Arc;
#[cfg(feature = "huggingface")]
use tokenizers::Tokenizer;
#[cfg(feature = "huggingface")]
use tokio::sync::RwLock;

#[cfg(feature = "huggingface")]
/// HuggingFace model instance
pub struct HuggingFaceModel {
	model: BertModel,
	tokenizer: Tokenizer,
	device: Device,
}

#[cfg(feature = "huggingface")]
impl HuggingFaceModel {
	/// Load a SentenceTransformer model from HuggingFace Hub
	pub async fn load(model_name: &str) -> Result<Self> {
		let device = Device::Cpu; // Use CPU for now, can be extended to support GPU

		// Use our custom cache directory for consistency with FastEmbed
		// Set HF_HOME environment variable to control where models are downloaded
		let cache_dir = crate::storage::get_huggingface_cache_dir()
			.context("Failed to get HuggingFace cache directory")?;

		// Set the HuggingFace cache directory via environment variable
		std::env::set_var("HF_HOME", &cache_dir);

		// Download model files from HuggingFace Hub with proper error handling
		let api = Api::new().context("Failed to initialize HuggingFace API")?;
		let repo = api.repo(Repo::new(model_name.to_string(), RepoType::Model));

		// Download required files with enhanced error handling
		let config_path = repo
			.get("config.json")
			.await
			.with_context(|| format!("Failed to download config.json for model: {}", model_name))?;

		// Try different tokenizer formats for better compatibility
		let tokenizer_path = if let Ok(path) = repo.get("tokenizer.json").await {
			path
		} else if let Ok(path) = repo.get("tokenizer_config.json").await {
			path
		} else {
			return Err(anyhow::anyhow!(
				"Could not find tokenizer files (tokenizer.json or tokenizer_config.json) for model: {}. \
				This model may not be compatible or may be missing required files.",
				model_name
			));
		};

		// Try different weight file formats
		let weights_path = if let Ok(path) = repo.get("model.safetensors").await {
			path
		} else if let Ok(path) = repo.get("pytorch_model.bin").await {
			path
		} else {
			return Err(anyhow::anyhow!(
				"Could not find model weights in safetensors or pytorch format"
			));
		};

		// Load configuration
		let config_content = std::fs::read_to_string(config_path)?;
		let config: BertConfig = serde_json::from_str(&config_content)?;

		// Load tokenizer
		let tokenizer = Tokenizer::from_file(tokenizer_path)
			.map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

		// Load model weights - only support safetensors for now
		let weights = if weights_path.to_string_lossy().ends_with(".safetensors") {
			candle_core::safetensors::load(&weights_path, &device)?
		} else {
			return Err(anyhow::anyhow!("PyTorch .bin format not supported in this implementation. Please use a model with safetensors format."));
		};

		let var_builder = VarBuilder::from_tensors(weights, DType::F32, &device);

		// Create model
		let model = BertModel::load(var_builder, &config)?;

		Ok(Self {
			model,
			tokenizer,
			device,
		})
	}

	/// Generate embeddings for a single text
	pub fn encode(&self, text: &str) -> Result<Vec<f32>> {
		self.encode_batch(&[text.to_string()])
			.map(|embeddings| embeddings.into_iter().next().unwrap_or_default())
	}

	/// Generate embeddings for multiple texts
	pub fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
		let mut all_embeddings = Vec::new();

		for text in texts {
			// Tokenize input - convert String to &str
			let encoding = self
				.tokenizer
				.encode(text.as_str(), true)
				.map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

			let tokens = encoding.get_ids();
			let token_ids = Tensor::new(tokens, &self.device)?.unsqueeze(0)?; // Add batch dimension

			// Create attention mask (all 1s for valid tokens)
			let attention_mask = Tensor::ones((1, tokens.len()), DType::U8, &self.device)?;

			// Run through model - BertModel.forward takes 3 arguments: input_ids, attention_mask, token_type_ids
			let output = self.model.forward(&token_ids, &attention_mask, None)?;

			// Apply mean pooling to get sentence embedding
			let embeddings = self.mean_pooling(&output, &attention_mask)?;

			// Normalize embeddings
			let normalized = self.normalize(&embeddings)?;

			// Convert to Vec<f32>
			let embedding_vec = normalized.to_vec1::<f32>()?;
			all_embeddings.push(embedding_vec);
		}

		Ok(all_embeddings)
	}

	/// Mean pooling operation
	fn mean_pooling(&self, hidden_states: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
		// Convert attention mask to f32 and expand dimensions
		let attention_mask = attention_mask.to_dtype(DType::F32)?;
		let attention_mask = attention_mask.unsqueeze(2)?; // (batch_size, seq_len, 1)

		// Apply attention mask to hidden states
		let masked_hidden_states = hidden_states.mul(&attention_mask)?;

		// Sum along sequence dimension
		let sum_hidden_states = masked_hidden_states.sum(1)?; // (batch_size, hidden_size)

		// Sum attention mask to get actual sequence lengths
		let sum_mask = attention_mask.sum(1)?; // (batch_size, 1)

		// Compute mean
		let mean_pooled = sum_hidden_states.div(&sum_mask)?;

		Ok(mean_pooled)
	}

	/// Normalize embeddings to unit vectors
	fn normalize(&self, embeddings: &Tensor) -> Result<Tensor> {
		let norm = embeddings.sqr()?.sum_keepdim(1)?.sqrt()?;
		Ok(embeddings.div(&norm)?)
	}
}

#[cfg(feature = "huggingface")]
// Global cache for loaded models using async-compatible RwLock
lazy_static::lazy_static! {
	static ref MODEL_CACHE: Arc<RwLock<HashMap<String, Arc<HuggingFaceModel>>>> =
		Arc::new(RwLock::new(HashMap::new()));
}

#[cfg(feature = "huggingface")]
/// HuggingFace provider implementation
pub struct HuggingFaceProvider;

#[cfg(feature = "huggingface")]
impl HuggingFaceProvider {
	/// Get or load a model from cache
	async fn get_model(model_name: &str) -> Result<Arc<HuggingFaceModel>> {
		{
			let cache = MODEL_CACHE.read().await;
			if let Some(model) = cache.get(model_name) {
				return Ok(model.clone());
			}
		}

		// Model not in cache, load it
		let model = HuggingFaceModel::load(model_name)
			.await
			.with_context(|| format!("Failed to load HuggingFace model: {}", model_name))?;

		let model_arc = Arc::new(model);

		// Add to cache
		{
			let mut cache = MODEL_CACHE.write().await;
			cache.insert(model_name.to_string(), model_arc.clone());
		}

		Ok(model_arc)
	}

	/// Generate embeddings for a single text
	pub async fn generate_embeddings(contents: &str, model: &str) -> Result<Vec<f32>> {
		let model_instance = Self::get_model(model).await?;

		// Run encoding in a blocking task to avoid blocking async runtime
		let contents = contents.to_string();
		let result =
			tokio::task::spawn_blocking(move || model_instance.encode(&contents)).await??;

		Ok(result)
	}

	/// Generate batch embeddings for multiple texts
	pub async fn generate_embeddings_batch(
		texts: Vec<String>,
		model: &str,
	) -> Result<Vec<Vec<f32>>> {
		let model_instance = Self::get_model(model).await?;

		// Run encoding in a blocking task to avoid blocking async runtime
		let result =
			tokio::task::spawn_blocking(move || model_instance.encode_batch(&texts)).await??;

		Ok(result)
	}
}

// Stubs for when huggingface feature is disabled
#[cfg(not(feature = "huggingface"))]
use anyhow::Result;

#[cfg(not(feature = "huggingface"))]
pub struct HuggingFaceProvider;

#[cfg(not(feature = "huggingface"))]
impl HuggingFaceProvider {
	pub async fn generate_embeddings(_contents: &str, _model: &str) -> Result<Vec<f32>> {
		Err(anyhow::anyhow!(
			"HuggingFace support is not compiled in. Please rebuild with --features huggingface"
		))
	}

	pub async fn generate_embeddings_batch(
		_texts: Vec<String>,
		_model: &str,
	) -> Result<Vec<Vec<f32>>> {
		Err(anyhow::anyhow!(
			"HuggingFace support is not compiled in. Please rebuild with --features huggingface"
		))
	}
}
use super::super::types::InputType;
use super::EmbeddingProvider;

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
			let dimension = Self::get_model_dimension(model)?;
			Ok(Self {
				model_name: model.to_string(),
				dimension,
			})
		}
	}

	#[cfg(feature = "huggingface")]
	fn get_model_dimension(model: &str) -> Result<usize> {
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
