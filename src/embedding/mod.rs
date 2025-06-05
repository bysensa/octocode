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

pub mod provider;
#[cfg(test)]
mod tests;
pub mod types;

use crate::config::Config;
use anyhow::Result;
use tiktoken_rs::cl100k_base;

pub use provider::{create_embedding_provider_from_parts, EmbeddingProvider};
pub use types::*;

/// Generate embeddings based on configured provider (supports provider:model format)
pub async fn generate_embeddings(
	contents: &str,
	is_code: bool,
	config: &Config,
) -> Result<Vec<f32>> {
	// Get the model string from config
	let model_string = if is_code {
		&config.embedding.code_model
	} else {
		&config.embedding.text_model
	};

	// Parse provider and model from the string
	let (provider, model) = parse_provider_model(model_string);

	let provider_impl = create_embedding_provider_from_parts(&provider, &model)?;
	provider_impl.generate_embedding(contents).await
}

/// Count tokens in a text using tiktoken (cl100k_base tokenizer)
pub fn count_tokens(text: &str) -> usize {
	let bpe = cl100k_base().expect("Failed to load cl100k_base tokenizer");
	bpe.encode_with_special_tokens(text).len()
}

/// Split texts into batches respecting both count and token limits
pub fn split_texts_into_token_limited_batches(
	texts: Vec<String>,
	max_batch_size: usize,
	max_tokens_per_batch: usize,
) -> Vec<Vec<String>> {
	let mut batches = Vec::new();
	let mut current_batch = Vec::new();
	let mut current_token_count = 0;

	for text in texts {
		let text_tokens = count_tokens(&text);
		
		// If adding this text would exceed either limit, start a new batch
		if !current_batch.is_empty() && 
		   (current_batch.len() >= max_batch_size || 
		    current_token_count + text_tokens > max_tokens_per_batch) {
			batches.push(current_batch);
			current_batch = Vec::new();
			current_token_count = 0;
		}
		
		current_batch.push(text);
		current_token_count += text_tokens;
	}

	// Add the last batch if it's not empty
	if !current_batch.is_empty() {
		batches.push(current_batch);
	}

	batches
}

/// Generate batch embeddings based on configured provider (supports provider:model format)
/// Now includes token-aware batching
pub async fn generate_embeddings_batch(
	texts: Vec<String>,
	is_code: bool,
	config: &Config,
) -> Result<Vec<Vec<f32>>> {
	// Get the model string from config
	let model_string = if is_code {
		&config.embedding.code_model
	} else {
		&config.embedding.text_model
	};

	// Parse provider and model from the string
	let (provider, model) = parse_provider_model(model_string);

	let provider_impl = create_embedding_provider_from_parts(&provider, &model)?;
	
	// Split texts into token-limited batches
	let batches = split_texts_into_token_limited_batches(
		texts,
		config.index.embeddings_batch_size,
		config.index.embeddings_max_tokens_per_batch,
	);

	let mut all_embeddings = Vec::new();
	
	// Process each batch
	for batch in batches {
		let batch_embeddings = provider_impl.generate_embeddings_batch(batch).await?;
		all_embeddings.extend(batch_embeddings);
	}

	Ok(all_embeddings)
}

/// Calculate a unique hash for content including file path
pub fn calculate_unique_content_hash(contents: &str, file_path: &str) -> String {
	use sha2::{Digest, Sha256};
	let mut hasher = Sha256::new();
	hasher.update(contents.as_bytes());
	hasher.update(file_path.as_bytes());
	format!("{:x}", hasher.finalize())
}

/// Calculate content hash without file path
pub fn calculate_content_hash(contents: &str) -> String {
	use sha2::{Digest, Sha256};
	let mut hasher = Sha256::new();
	hasher.update(contents.as_bytes());
	format!("{:x}", hasher.finalize())
}
