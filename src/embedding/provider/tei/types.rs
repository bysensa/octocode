use std::collections::HashMap;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tei_backend::Pool;
use tei_core::tokenization::EncodingInput;

#[derive(Debug)]
pub(crate) enum Sequence {
	Single(String),
	Pair(String, String),
}

impl Sequence {
	pub(crate) fn count_chars(&self) -> usize {
		match self {
			Sequence::Single(s) => s.chars().count(),
			Sequence::Pair(s1, s2) => s1.chars().count() + s2.chars().count(),
		}
	}
}

impl From<Sequence> for EncodingInput {
	fn from(value: Sequence) -> Self {
		match value {
			Sequence::Single(s) => Self::Single(s),
			Sequence::Pair(s1, s2) => Self::Dual(s1, s2),
		}
	}
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelConfig {
	pub(crate) architectures: Vec<String>,
	pub(crate) model_type: String,
	#[serde(alias = "n_positions")]
	pub(crate) max_position_embeddings: usize,
	#[serde(alias = "hidden_size")]
	pub(crate) hidden_size: usize,
	#[serde(default)]
	pub(crate) pad_token_id: usize,
	pub(crate) id2label: Option<HashMap<String, String>>,
	pub(crate) label2id: Option<HashMap<String, usize>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct PoolConfig {
	pooling_mode_cls_token: bool,
	pooling_mode_mean_tokens: bool,
	#[serde(default)]
	pooling_mode_lasttoken: bool,
}

impl TryFrom<PoolConfig> for Pool {
	type Error = anyhow::Error;

	fn try_from(config: PoolConfig) -> std::result::Result<Self, Self::Error> {
		if config.pooling_mode_cls_token {
			return Ok(Pool::Cls);
		}
		if config.pooling_mode_mean_tokens {
			return Ok(Pool::Mean);
		}
		if config.pooling_mode_lasttoken {
			return Ok(Pool::LastToken);
		}
		Err(anyhow!("Pooling config {config:?} is not supported"))
	}
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Info {
	/// Model info
	pub(crate) model_id: String,
	pub(crate) model_sha: Option<String>,
	pub(crate) model_dtype: String,
	pub(crate) model_type: ModelTypeWrapper,
	/// Router Parameters
	pub(crate) max_concurrent_requests: usize,
	pub(crate) max_input_length: usize,
	pub(crate) max_batch_tokens: usize,
	pub(crate) max_batch_requests: Option<usize>,
	pub(crate) max_client_batch_size: usize,
	pub(crate) auto_truncate: bool,
	pub(crate) tokenization_workers: usize,
	/// Router Info
	pub(crate) version: &'static str,
	pub(crate) sha: Option<&'static str>,
	pub(crate) docker_label: Option<&'static str>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct STConfig {
	pub(crate) max_seq_length: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NewSTConfig {
	pub(crate) prompts: Option<HashMap<String, String>>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EmbeddingModel {
	pub(crate) pooling: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ClassifierModel {
	pub(crate) id2label: HashMap<String, String>,
	pub(crate) label2id: HashMap<String, usize>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ModelTypeWrapper {
	Classifier(ClassifierModel),
	Embedding(EmbeddingModel),
	Reranker(ClassifierModel),
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Eq, Default)]
pub(crate) enum TruncationDirection {
	Left,
	#[default]
	Right,
}

impl From<TruncationDirection> for tokenizers::TruncationDirection {
	fn from(value: TruncationDirection) -> Self {
		match value {
			TruncationDirection::Left => Self::Left,
			TruncationDirection::Right => Self::Right,
		}
	}
}

#[derive(Debug)]
pub(crate) enum InputType {
	String(String),
	Ids(Vec<u32>),
}

impl InputType {
	pub(crate) fn count_chars(&self) -> usize {
		match self {
			InputType::String(s) => s.chars().count(),
			InputType::Ids(v) => v.len(),
		}
	}
}

impl From<InputType> for EncodingInput {
	fn from(value: InputType) -> Self {
		match value {
			InputType::String(s) => Self::Single(s),
			InputType::Ids(v) => Self::Ids(v),
		}
	}
}

pub(crate) enum Input {
	Single(InputType),
	Batch(Vec<InputType>),
}
