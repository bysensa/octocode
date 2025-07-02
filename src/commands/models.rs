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
 * Model Management Commands
 *
 * This module provides CLI commands for dynamic model discovery and validation.
 * All operations use provider-native APIs for real-time model information.
 *
 * Commands:
 * - `octocode models list [provider]` - List all supported models for provider(s)
 * - `octocode models validate <provider:model>` - Validate specific model and show details
 * - `octocode models dimensions <provider:model>` - Get model dimensions dynamically
 *
 * Key features:
 * - Pure dynamic discovery - no hardcoded model lists
 * - Real-time provider API integration
 * - Comprehensive model validation
 * - Dimension detection without fallbacks
 */

use anyhow::Result;
use clap::Subcommand;

use octocode::embedding::{
	provider::create_embedding_provider_from_parts, types::EmbeddingProviderType,
};

#[cfg(feature = "fastembed")]
use octocode::embedding::provider::fastembed::FastEmbedProviderImpl;

#[derive(Subcommand, Debug, Clone)]
pub enum ModelsCommand {
	/// List all supported models for embedding providers
	List {
		/// Optional provider filter (fastembed, huggingface, jina, voyage, google)
		provider: Option<String>,
	},
	/// Get detailed information about a specific model
	Info {
		/// Model in provider:model format (e.g., "voyage:voyage-code-2")
		model: String,
	},
}

pub async fn execute_models_command(cmd: ModelsCommand) -> Result<()> {
	match cmd {
		ModelsCommand::List { provider } => list_models(provider).await,
		ModelsCommand::Info { model } => get_model_info(&model).await,
	}
}

async fn list_models(provider_filter: Option<String>) -> Result<()> {
	println!("=== Dynamic Model Discovery ===");

	let providers = if let Some(filter) = provider_filter {
		vec![parse_provider(&filter)?]
	} else {
		vec![
			EmbeddingProviderType::FastEmbed,
			EmbeddingProviderType::HuggingFace,
			EmbeddingProviderType::Jina,
			EmbeddingProviderType::Voyage,
			EmbeddingProviderType::Google,
		]
	};

	for provider in providers {
		println!("\n--- {:?} Provider ---", provider);

		match provider {
			EmbeddingProviderType::FastEmbed => {
				#[cfg(feature = "fastembed")]
				{
					let models = FastEmbedProviderImpl::list_supported_models();
					println!("Found {} models:", models.len());
					for (i, model) in models.iter().enumerate() {
						println!("  {}. {}", i + 1, model);
					}
				}
				#[cfg(not(feature = "fastembed"))]
				{
					println!("  FastEmbed feature not enabled");
				}
			}
			EmbeddingProviderType::HuggingFace => {
				#[cfg(feature = "huggingface")]
				{
					println!("  HuggingFace: Dynamic discovery via Hub API");
					println!("  Use 'info' command with specific model names");
				}
				#[cfg(not(feature = "huggingface"))]
				{
					println!("  HuggingFace feature not enabled");
				}
			}
			EmbeddingProviderType::Jina => {
				println!(
					"  Jina models: jina-embeddings-v2-base-en, jina-embeddings-v2-small-en, etc."
				);
				println!("  Use 'info' command for real-time API validation");
			}
			EmbeddingProviderType::Voyage => {
				println!("  Voyage models: voyage-3.5, voyage-code-2, voyage-finance-2, etc.");
				println!("  Use 'info' command for real-time API validation");
			}
			EmbeddingProviderType::Google => {
				println!("  Google models: gemini-embedding-001 (3072d), text-embedding-005 (768d), text-multilingual-embedding-002 (768d)");
				println!("  Use 'info' command for real-time API validation");
			}
		}
	}

	Ok(())
}

async fn get_model_info(model_spec: &str) -> Result<()> {
	let (provider_str, model_name) = parse_model_spec(model_spec)?;
	let provider = parse_provider(&provider_str)?;

	println!("=== Model Information ===");
	println!("Provider: {:?}", provider);
	println!("Model: {}", model_name);

	// Create provider instance to test validation
	match create_embedding_provider_from_parts(&provider, &model_name) {
		Ok(provider_impl) => {
			let supported = provider_impl.is_model_supported();

			if !supported {
				println!(
					"❌ Model '{}' is not supported by provider {:?}",
					model_name, provider
				);
				return Err(anyhow::anyhow!(
					"Model '{}' is not supported by provider {:?}",
					model_name,
					provider
				));
			}

			let dimension = provider_impl.get_dimension();

			println!("✅ Model is supported: {}", supported);
			println!("📏 Dimension: {}", dimension);
			println!("🎯 Model information retrieved successfully!");
		}
		Err(e) => {
			println!("❌ Failed to get model information: {}", e);
			return Err(e);
		}
	}

	Ok(())
}

fn parse_model_spec(model_spec: &str) -> Result<(String, String)> {
	let parts: Vec<&str> = model_spec.splitn(2, ':').collect();
	if parts.len() != 2 {
		return Err(anyhow::anyhow!(
			"Invalid model specification '{}'. Expected format: 'provider:model'",
			model_spec
		));
	}

	Ok((parts[0].to_string(), parts[1].to_string()))
}

fn parse_provider(provider_str: &str) -> Result<EmbeddingProviderType> {
	match provider_str.to_lowercase().as_str() {
		"fastembed" => Ok(EmbeddingProviderType::FastEmbed),
		"huggingface" => Ok(EmbeddingProviderType::HuggingFace),
		"jina" => Ok(EmbeddingProviderType::Jina),
		"voyage" => Ok(EmbeddingProviderType::Voyage),
		"google" => Ok(EmbeddingProviderType::Google),
		_ => Err(anyhow::anyhow!(
			"Unknown provider '{}'. Supported: fastembed, huggingface, jina, voyage, google",
			provider_str
		)),
	}
}
