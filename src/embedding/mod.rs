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

/// Generate batch embeddings based on configured provider (supports provider:model format)
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
	let embeddings = provider_impl.generate_embeddings_batch(texts).await?;

	Ok(embeddings)
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
