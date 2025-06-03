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
* SentenceTransformer Provider Implementation
*
* This module provides local embedding generation using HuggingFace models via the Candle library.
* It supports any BERT-based model with safetensors format from the HuggingFace Hub.
*
* Key features:
* - Automatic model downloading and caching
* - Local CPU-based inference (GPU support can be added)
* - Thread-safe model cache for efficient reuse
* - Mean pooling and L2 normalization for sentence embeddings
* - Full compatibility with provider:model syntax
*
* Usage:
* - Set provider: `octocode config --embedding-provider sentencetransformer`
* - Set models: `octocode config --code-embedding-model "sentencetransformer:microsoft/codebert-base"`
* - Popular models: microsoft/codebert-base, sentence-transformers/all-mpnet-base-v2
*
* Models are automatically downloaded to the system cache directory and reused across sessions.
*/

// When sentence-transformer feature is enabled
#[cfg(feature = "sentence-transformer")]
use anyhow::{Context, Result};
#[cfg(feature = "sentence-transformer")]
use candle_core::{DType, Device, Tensor};
#[cfg(feature = "sentence-transformer")]
use candle_nn::VarBuilder;
#[cfg(feature = "sentence-transformer")]
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
#[cfg(feature = "sentence-transformer")]
use hf_hub::{api::tokio::Api, Repo, RepoType};
#[cfg(feature = "sentence-transformer")]
use std::collections::HashMap;
#[cfg(feature = "sentence-transformer")]
use std::sync::Arc;
#[cfg(feature = "sentence-transformer")]
use tokenizers::Tokenizer;
#[cfg(feature = "sentence-transformer")]
use tokio::sync::RwLock;

#[cfg(feature = "sentence-transformer")]
/// SentenceTransformer model instance
pub struct SentenceTransformerModel {
	model: BertModel,
	tokenizer: Tokenizer,
	device: Device,
}

#[cfg(feature = "sentence-transformer")]
impl SentenceTransformerModel {
	/// Load a SentenceTransformer model from HuggingFace Hub
	pub async fn load(model_name: &str) -> Result<Self> {
		let device = Device::Cpu; // Use CPU for now, can be extended to support GPU

		// Use our custom cache directory for consistency with FastEmbed
		// Set HF_HOME environment variable to control where models are downloaded
		let cache_dir = crate::storage::get_sentencetransformer_cache_dir()
			.context("Failed to get SentenceTransformer cache directory")?;

		// Set the HuggingFace cache directory via environment variable
		std::env::set_var("HF_HOME", &cache_dir);

		// Download model files from HuggingFace Hub
		let api = Api::new()?;
		let repo = api.repo(Repo::with_revision(
			model_name.to_string(),
			RepoType::Model,
			"main".to_string(),
		));

		// Download required files
		let config_path = repo.get("config.json").await?;
		let tokenizer_path = repo.get("tokenizer.json").await?;

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

#[cfg(feature = "sentence-transformer")]
// Global cache for loaded models using async-compatible RwLock
lazy_static::lazy_static! {
	static ref MODEL_CACHE: Arc<RwLock<HashMap<String, Arc<SentenceTransformerModel>>>> =
		Arc::new(RwLock::new(HashMap::new()));
}

#[cfg(feature = "sentence-transformer")]
/// SentenceTransformer provider implementation
pub struct SentenceTransformerProvider;

#[cfg(feature = "sentence-transformer")]
impl SentenceTransformerProvider {
	/// Get or load a model from cache
	async fn get_model(model_name: &str) -> Result<Arc<SentenceTransformerModel>> {
		{
			let cache = MODEL_CACHE.read().await;
			if let Some(model) = cache.get(model_name) {
				return Ok(model.clone());
			}
		}

		// Model not in cache, load it
		let model = SentenceTransformerModel::load(model_name)
			.await
			.with_context(|| format!("Failed to load SentenceTransformer model: {}", model_name))?;

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

// Stubs for when sentence-transformer feature is disabled
#[cfg(not(feature = "sentence-transformer"))]
use anyhow::Result;

#[cfg(not(feature = "sentence-transformer"))]
pub struct SentenceTransformerProvider;

#[cfg(not(feature = "sentence-transformer"))]
impl SentenceTransformerProvider {
	pub async fn generate_embeddings(_contents: &str, _model: &str) -> Result<Vec<f32>> {
		Err(anyhow::anyhow!("SentenceTransformer support is not compiled in. Please rebuild with --features sentence-transformer"))
	}

	pub async fn generate_embeddings_batch(
		_texts: Vec<String>,
		_model: &str,
	) -> Result<Vec<Vec<f32>>> {
		Err(anyhow::anyhow!("SentenceTransformer support is not compiled in. Please rebuild with --features sentence-transformer"))
	}
}
