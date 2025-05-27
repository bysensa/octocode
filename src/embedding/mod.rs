pub mod provider;
pub mod types;
#[cfg(test)]
mod tests;

use anyhow::Result;
use crate::config::Config;

pub use types::*;
pub use provider::*;

/// Generate embeddings based on configured provider (supports provider:model format)
pub async fn generate_embeddings(contents: &str, is_code: bool, config: &Config) -> Result<Vec<f32>> {
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
pub async fn generate_embeddings_batch(texts: Vec<String>, is_code: bool, config: &Config) -> Result<Vec<Vec<f32>>> {
	// Get the model string from config
	let model_string = if is_code { 
		&config.embedding.code_model 
	} else { 
		&config.embedding.text_model 
	};

	// Parse provider and model from the string
	let (provider, model) = parse_provider_model(model_string);
	
	let provider_impl = create_embedding_provider_from_parts(&provider, &model)?;
	provider_impl.generate_embeddings_batch(texts).await
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
