use anyhow::{anyhow, Context, Result};
use bon::{bon, builder};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tei_backend::{DType, ModelType, Pool};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use hf_hub::api::tokio::ApiBuilder;
use hf_hub::{Repo, RepoType};

use tei_core::download::{download_artifacts, ST_CONFIG_NAMES};
use tei_core::infer::Infer;
use tei_core::queue::Queue;
use tei_core::tokenization::Tokenization;
use tokenizers::processors::sequence::Sequence;
use tokenizers::processors::template::TemplateProcessing;
use tokenizers::{PostProcessorWrapper, Tokenizer};

pub(crate) use decode::*;
pub(crate) use embed::*;
pub(crate) use health::*;
pub(crate) use predict::*;
pub(crate) use rerank::*;
pub(crate) use similarity::*;
pub(crate) use tokenize::*;
pub(crate) use types::*;

mod decode;
mod embed;
mod health;
mod predict;
mod rerank;
mod similarity;
mod tokenize;
mod types;

pub(crate) struct Tei {
	pub(self) infer: Arc<Infer>,
	pub(self) info: Arc<Info>,
	pub(self) dim: usize,
}

#[bon]
impl Tei {
	pub(crate) async fn from_model(model: impl Into<String>) -> anyhow::Result<Self> {
		Self::builder()
			.model_id(model)
			.auto_truncate(false)
			.tokenization_workers(num_cpus::get())
			.max_concurrent_requests(num_cpus::get())
			.max_batch_tokens(16384)
			.max_client_batch_size(32)
			.build()
			.await
	}

	#[allow(clippy::too_many_arguments)]
	#[builder(on(String, into))]
	pub(crate) async fn new(
		model_id: String,
		revision: Option<String>,
		tokenization_workers: Option<usize>,
		dtype: Option<DType>,
		pooling: Option<Pool>,
		max_concurrent_requests: usize,
		max_batch_tokens: usize,
		max_batch_requests: Option<usize>,
		max_client_batch_size: usize,
		auto_truncate: bool,
		default_prompt: Option<String>,
		default_prompt_name: Option<String>,
		hf_token: Option<String>,
		uds_path: Option<String>,
		huggingface_hub_cache: Option<String>,
	) -> anyhow::Result<Self> {
		let model_id_path = Path::new(&model_id);
		let (model_root, api_repo) = if model_id_path.exists() && model_id_path.is_dir() {
			// Using a local model
			(model_id_path.to_path_buf(), None)
		} else {
			let mut builder = ApiBuilder::from_env()
				.with_progress(false)
				.with_token(hf_token);

			if let Some(cache_dir) = huggingface_hub_cache {
				builder = builder.with_cache_dir(cache_dir.into());
			}

			if let Ok(origin) = std::env::var("HF_HUB_USER_AGENT_ORIGIN") {
				builder = builder.with_user_agent("origin", origin.as_str());
			}

			let api = builder.build().unwrap();
			let api_repo = api.repo(Repo::with_revision(
				model_id.clone(),
				RepoType::Model,
				revision.clone().unwrap_or("main".to_string()),
			));

			// Download model from the Hub
			(
				download_artifacts(&api_repo, pooling.is_none())
					.await
					.context("Could not download model artifacts")?,
				Some(api_repo),
			)
		};

		// Load config
		let config_path = model_root.join("config.json");
		let config = fs::read_to_string(config_path).context("`config.json` not found")?;
		let config: ModelConfig =
			serde_json::from_str(&config).context("Failed to parse `config.json`")?;

		let model_dim = config.hidden_size;

		// Set model type from config
		let backend_model_type = get_backend_model_type(&config, &model_root, pooling)?;

		// Info model type
		let model_type = match &backend_model_type {
			tei_backend::ModelType::Classifier => {
				let id2label = config
					.id2label
					.context("`config.json` does not contain `id2label`")?;
				let n_classes = id2label.len();
				let classifier_model = ClassifierModel {
					id2label,
					label2id: config
						.label2id
						.context("`config.json` does not contain `label2id`")?,
				};
				if n_classes > 1 {
					ModelTypeWrapper::Classifier(classifier_model)
				} else {
					ModelTypeWrapper::Reranker(classifier_model)
				}
			}
			tei_backend::ModelType::Embedding(pool) => {
				ModelTypeWrapper::Embedding(EmbeddingModel {
					pooling: pool.to_string(),
				})
			}
		};

		// Load tokenizer
		let tokenizer_path = model_root.join("tokenizer.json");
		let mut tokenizer = Tokenizer::from_file(tokenizer_path).expect(
			"tokenizer.json not found. text-embeddings-inference only supports fast tokenizers",
		);
		tokenizer.with_padding(None);
		// Qwen2 updates the post processor manually instead of into the tokenizer.json...
		// https://huggingface.co/Alibaba-NLP/gte-Qwen2-1.5B-instruct/blob/main/tokenization_qwen.py#L246
		if config.model_type == "qwen2" {
			let template = TemplateProcessing::builder()
				.try_single("$A:0 <|endoftext|>:0")
				.unwrap()
				.try_pair("$A:0 <|endoftext|>:0 $B:1 <|endoftext|>:1")
				.unwrap()
				.special_tokens(vec![("<|endoftext|>", 151643)])
				.build()
				.unwrap();
			match tokenizer.get_post_processor() {
				None => tokenizer.with_post_processor(Some(template)),
				Some(post_processor) => {
					let post_processor = Sequence::new(vec![
						post_processor.clone(),
						PostProcessorWrapper::Template(template),
					]);
					tokenizer.with_post_processor(Some(post_processor))
				}
			};
		}

		// Position IDs offset. Used for Roberta and camembert.
		let position_offset = if &config.model_type == "xlm-roberta"
			|| &config.model_type == "camembert"
			|| &config.model_type == "roberta"
		{
			config.pad_token_id + 1
		} else {
			0
		};

		// Try to load ST Config
		let mut st_config: Option<STConfig> = None;
		for name in ST_CONFIG_NAMES {
			let config_path = model_root.join(name);
			if let Ok(config) = fs::read_to_string(config_path) {
				st_config = Some(
					serde_json::from_str(&config).context(format!("Failed to parse `{}`", name))?,
				);
				break;
			}
		}
		let max_input_length = match st_config {
			Some(config) => config.max_seq_length,
			None => {
				tracing::warn!("Could not find a Sentence Transformers config");
				config.max_position_embeddings - position_offset
			}
		};
		tracing::info!("Maximum number of tokens per request: {max_input_length}");

		let tokenization_workers = tokenization_workers.unwrap_or_else(num_cpus::get);

		// Try to load new ST Config
		let mut new_st_config: Option<NewSTConfig> = None;
		let config_path = model_root.join("config_sentence_transformers.json");
		if let Ok(config) = fs::read_to_string(config_path) {
			new_st_config = Some(
				serde_json::from_str(&config)
					.context("Failed to parse `config_sentence_transformers.json`")?,
			);
		}
		let prompts = new_st_config.and_then(|c| c.prompts);
		let default_prompt = if let Some(default_prompt_name) = default_prompt_name.as_ref() {
			match &prompts {
				None => {
					anyhow::bail!(format!("`default-prompt-name` is set to `{default_prompt_name}` but no prompts were found in the Sentence Transformers configuration"));
				}
				Some(prompts) if !prompts.contains_key(default_prompt_name) => {
					anyhow::bail!(format!("`default-prompt-name` is set to `{default_prompt_name}` but it was not found in the Sentence Transformers prompts. Available prompts: {:?}", prompts.keys()));
				}
				Some(prompts) => prompts.get(default_prompt_name).cloned(),
			}
		} else {
			default_prompt
		};

		// Tokenization logic
		let tokenization = Tokenization::new(
			tokenization_workers,
			tokenizer,
			max_input_length,
			position_offset,
			default_prompt,
			prompts,
		);

		// Get dtype
		let dtype = dtype.unwrap_or_default();

		// Create backend
		tracing::info!("Starting model backend");
		let backend = tei_backend::Backend::new(
			model_root,
			api_repo,
			dtype.clone(),
			backend_model_type,
			uds_path.unwrap_or("/tmp/text-embeddings-inference-server".to_string()),
			None,
			"tei".into(),
		)
		.await
		.context("Could not create backend")?;
		backend
			.health()
			.await
			.context("Model backend is not healthy")?;

		tracing::info!("Warming up model");
		backend
			.warmup(max_input_length, max_batch_tokens, max_batch_requests)
			.await
			.context("Model backend is not healthy")?;

		let max_batch_requests = backend
			.max_batch_size
			.inspect(|&s| {
				tracing::warn!("Backend does not support a batch size > {s}");
				tracing::warn!("forcing `max_batch_requests={s}`");
			})
			.or(max_batch_requests);

		// Queue logic
		let queue = Queue::new(
			backend.padded_model,
			max_batch_tokens,
			max_batch_requests,
			max_concurrent_requests,
		);

		// Create infer task
		let infer = Infer::new(tokenization, queue, max_concurrent_requests, backend);

		// Endpoint info
		let info = Info {
			model_id,
			model_sha: revision,
			model_dtype: dtype.to_string(),
			model_type,
			max_concurrent_requests,
			max_input_length,
			max_batch_tokens,
			tokenization_workers,
			max_batch_requests,
			max_client_batch_size,
			auto_truncate,
			version: env!("CARGO_PKG_VERSION"),
			sha: option_env!("VERGEN_GIT_SHA"),
			docker_label: option_env!("DOCKER_LABEL"),
		};

		Ok(Self {
			dim: model_dim,
			infer: Arc::new(infer),
			info: Arc::new(info),
		})
	}
}

fn get_backend_model_type(
	config: &ModelConfig,
	model_root: &Path,
	pooling: Option<Pool>,
) -> Result<ModelType> {
	for arch in &config.architectures {
		// Edge case affecting `Alibaba-NLP/gte-multilingual-base` and possibly other fine-tunes of
		// the same base model. More context at https://huggingface.co/Alibaba-NLP/gte-multilingual-base/discussions/7
		if arch == "NewForTokenClassification"
			&& (config.id2label.is_none() | config.label2id.is_none())
		{
			tracing::warn!("Provided `--model-id` is likely an AlibabaNLP GTE model, but the `config.json` contains the architecture `NewForTokenClassification` but it doesn't contain the `id2label` and `label2id` mapping, so `NewForTokenClassification` architecture will be ignored.");
			continue;
		}

		if Some(Pool::Splade) == pooling && arch.ends_with("MaskedLM") {
			return Ok(ModelType::Embedding(Pool::Splade));
		} else if arch.ends_with("Classification") {
			if pooling.is_some() {
				tracing::warn!(
					"`--pooling` arg is set but model is a classifier. Ignoring `--pooling` arg."
				);
			}
			return Ok(ModelType::Classifier);
		}
	}

	if Some(Pool::Splade) == pooling {
		return Err(anyhow!(
			"Splade pooling is not supported: model is not a ForMaskedLM model"
		));
	}

	// Set pooling
	let pool = match pooling {
		Some(pool) => pool,
		None => {
			// Load pooling config
			let config_path = model_root.join("1_Pooling/config.json");

			match fs::read_to_string(config_path) {
				Ok(config) => {
					let config: PoolConfig = serde_json::from_str(&config)
						.context("Failed to parse `1_Pooling/config.json`")?;
					Pool::try_from(config)?
				}
				Err(err) => {
					if !config.model_type.to_lowercase().contains("bert") {
						return Err(err).context("The `--pooling` arg is not set and we could not find a pooling configuration (`1_Pooling/config.json`) for this model.");
					}
					tracing::warn!("The `--pooling` arg is not set and we could not find a pooling configuration (`1_Pooling/config.json`) for this model but the model is a BERT variant. Defaulting to `CLS` pooling.");
					Pool::Cls
				}
			}
		}
	};
	Ok(ModelType::Embedding(pool))
}

/// Init logging using env variables LOG_LEVEL and LOG_FORMAT:
///     - otlp_endpoint is an optional URL to an Open Telemetry collector
///     - LOG_LEVEL may be TRACE, DEBUG, INFO, WARN or ERROR (default to INFO)
pub fn init_logging(json_output: bool, disable_spans: bool) -> bool {
	let mut layers = Vec::new();

	// STDOUT/STDERR layer
	let fmt_layer = tracing_subscriber::fmt::layer()
		.with_file(true)
		.with_line_number(true);

	let fmt_layer = match json_output {
		true => fmt_layer
			.json()
			.flatten_event(true)
			.with_current_span(!disable_spans)
			.with_span_list(!disable_spans)
			.boxed(),
		false => fmt_layer.boxed(),
	};
	layers.push(fmt_layer);

	// OpenTelemetry tracing layer
	let global_tracer = false;
	// Filter events with LOG_LEVEL
	let env_filter =
		EnvFilter::try_from_env("LOG_LEVEL").unwrap_or_else(|_| EnvFilter::new("info"));

	tracing_subscriber::registry()
		.with(env_filter)
		.with(layers)
		.init();
	global_tracer
}

#[async_trait::async_trait]
impl super::EmbeddingProvider for Tei {
	async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
		let input = Input::Single(InputType::String(text.to_string()));
		let res = self.embed(input, false, None, None, None).await?;
		let embeddings = res.0.into_iter().nth(0);
		embeddings.ok_or(anyhow!("TEI: no embeddings"))
	}

	async fn generate_embeddings_batch(
		&self,
		texts: Vec<String>,
		input_type: crate::embedding::InputType,
	) -> Result<Vec<Vec<f32>>> {
		let input: Vec<InputType> = texts.into_iter().map(InputType::String).collect();
		let input = Input::Batch(input);
		let res = self
			.embed(
				input,
				false,
				None,
				None,
				input_type.as_api_str().map(Into::into),
			)
			.await?;
		let embeddings = res.0;
		Ok(embeddings)
	}

	fn get_dimension(&self) -> usize {
		self.dim
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[tokio::test]
	async fn test_embedding() {
		let tei = Tei::builder()
			.auto_truncate(false)
			.max_batch_tokens(16384)
			.max_client_batch_size(32)
			.model_id("Qwen/Qwen3-Embedding-0.6B")
			.max_concurrent_requests(32)
			.build()
			.await
			.unwrap();

		let text = "What a wounderful day";
		let embeddings = tei
			.embed(
				Input::Single(InputType::String(text.into())),
				true,
				Some(false),
				None,
				None,
			)
			.await
			.unwrap();
		dbg!(embeddings);
	}
}
