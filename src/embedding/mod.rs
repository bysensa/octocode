pub mod provider;
pub mod types;

use anyhow::Result;
use crate::config::Config;

pub use types::*;
pub use provider::*;

/// Generate embeddings based on configured provider
pub async fn generate_embeddings(contents: &str, _is_code: bool, config: &Config) -> Result<Vec<f32>> {
	let provider = create_embedding_provider(config)?;
	provider.generate_embedding(contents).await
}

/// Generate batch embeddings based on configured provider
pub async fn generate_embeddings_batch(texts: Vec<String>, _is_code: bool, config: &Config) -> Result<Vec<Vec<f32>>> {
	let provider = create_embedding_provider(config)?;
	provider.generate_embeddings_batch(texts).await
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
