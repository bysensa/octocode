use anyhow::anyhow;
use futures::future::join_all;
use tei_core::{
	infer::{AllEmbeddingsInferResponse, PooledEmbeddingsInferResponse},
	TextEmbeddingsError,
};

use super::{
	types::{Input, TruncationDirection},
	Tei,
};

impl Tei {
	pub(crate) async fn embed(
		&self,
		input: Input,
		normalize: bool,
		truncate: Option<bool>,
		truncation_direction: Option<TruncationDirection>,
		prompt_name: Option<String>,
	) -> anyhow::Result<EmbedResult> {
		let truncate = truncate.unwrap_or(self.info.auto_truncate);
		let truncate_direction = truncation_direction.unwrap_or(TruncationDirection::Right);

		let result = match input {
			Input::Single(input) => {
				let permit = self
					.infer
					.try_acquire_permit()
					.map_err(|err| anyhow!(err))?;

				let response = self
					.infer
					.embed_pooled(
						input,
						truncate,
						truncate_direction.into(),
						prompt_name,
						normalize,
						permit,
					)
					.await
					.map_err(|err| anyhow!(err))?;

				EmbedResult(vec![response.results])
			}
			Input::Batch(inputs) => {
				if inputs.is_empty() {
					let message = "`inputs` cannot be empty".to_string();
					tracing::error!("{message}");
					let err = anyhow!(message);
					Err(err)?;
				}

				let batch_size = inputs.len();
				if batch_size > self.info.max_client_batch_size {
					let message = format!(
						"batch size {batch_size} > maximum allowed batch size {}",
						self.info.max_client_batch_size
					);
					tracing::error!("{message}");
					let err = anyhow!(message);
					Err(err)?;
				}

				let mut futures = Vec::with_capacity(batch_size);

				for input in inputs {
					let local_infer = self.infer.clone();
					let prompt_name = prompt_name.clone();
					futures.push(async move {
						let permit = local_infer.acquire_permit().await;
						local_infer
							.embed_pooled(
								input,
								truncate,
								truncate_direction.into(),
								prompt_name,
								normalize,
								permit,
							)
							.await
					})
				}
				let results = join_all(futures)
					.await
					.into_iter()
					.collect::<Result<Vec<PooledEmbeddingsInferResponse>, TextEmbeddingsError>>()
					.map_err(|err| anyhow!(err))?;

				let mut embeddings = Vec::with_capacity(batch_size);
				for r in results {
					embeddings.push(r.results);
				}

				EmbedResult(embeddings)
			}
		};

		tracing::info!("Success");

		Ok(result)
	}

	pub(crate) async fn embed_sparse(
		&self,
		input: Input,
		truncate: Option<bool>,
		truncation_direction: Option<TruncationDirection>,
		prompt_name: Option<String>,
	) -> anyhow::Result<EmbedSparseResult> {
		let sparsify = |values: Vec<f32>| {
			let mut sparse_values = Vec::with_capacity(values.len());
			for (index, value) in values.into_iter().enumerate() {
				if value != 0.0 {
					sparse_values.push(SparseValue { index, value });
				}
			}
			sparse_values
		};
		let truncate = truncate.unwrap_or(self.info.auto_truncate);
		let truncate_direction = truncation_direction.unwrap_or(TruncationDirection::Right);

		let result = match input {
			Input::Single(input) => {
				let permit = self
					.infer
					.try_acquire_permit()
					.map_err(|err| anyhow!(err))?;
				let response = self
					.infer
					.embed_sparse(
						input,
						truncate,
						truncate_direction.into(),
						prompt_name,
						permit,
					)
					.await
					.map_err(|err| anyhow!(err))?;

				EmbedSparseResult(vec![sparsify(response.results)])
			}
			Input::Batch(inputs) => {
				if inputs.is_empty() {
					let message = "`inputs` cannot be empty".to_string();
					tracing::error!("{message}");
					let err = anyhow!(message);
					Err(err)?;
				}

				let batch_size = inputs.len();
				if batch_size > self.info.max_client_batch_size {
					let message = format!(
						"batch size {batch_size} > maximum allowed batch size {}",
						self.info.max_client_batch_size
					);
					tracing::error!("{message}");
					let err = anyhow!(message);
					Err(err)?;
				}

				let mut futures = Vec::with_capacity(batch_size);

				for input in inputs {
					let local_infer = self.infer.clone();
					let prompt_name = prompt_name.clone();
					futures.push(async move {
						let permit = local_infer.acquire_permit().await;
						let response = local_infer
							.embed_sparse(
								input,
								truncate,
								truncate_direction.into(),
								prompt_name,
								permit,
							)
							.await?;
						Ok(sparsify(response.results))
					})
				}
				let results = join_all(futures)
					.await
					.into_iter()
					.collect::<Result<Vec<Vec<SparseValue>>, TextEmbeddingsError>>()
					.map_err(|err| anyhow!(err))?;

				let mut embeddings = Vec::with_capacity(batch_size);

				for r in results {
					embeddings.push(r);
				}

				EmbedSparseResult(embeddings)
			}
		};

		tracing::info!("Success");

		Ok(result)
	}

	pub(crate) async fn embed_all(
		&self,
		input: Input,
		truncate: Option<bool>,
		truncation_direction: Option<TruncationDirection>,
		prompt_name: Option<String>,
	) -> anyhow::Result<EmbedAllResult> {
		let truncate = truncate.unwrap_or(self.info.auto_truncate);
		let truncate_direction = truncation_direction.unwrap_or(TruncationDirection::Right);
		let result = match input {
			Input::Single(input) => {
				let permit = self
					.infer
					.try_acquire_permit()
					.map_err(|err| anyhow!(err))?;
				let response = self
					.infer
					.embed_all(
						input,
						truncate,
						truncate_direction.into(),
						prompt_name,
						permit,
					)
					.await
					.map_err(|err| anyhow!(err))?;
				EmbedAllResult(vec![response.results])
			}
			Input::Batch(inputs) => {
				if inputs.is_empty() {
					let message = "`inputs` cannot be empty".to_string();
					tracing::error!("{message}");
					let err = anyhow!(message);
					Err(err)?;
				}

				let batch_size = inputs.len();
				if batch_size > self.info.max_client_batch_size {
					let message = format!(
						"batch size {batch_size} > maximum allowed batch size {}",
						self.info.max_client_batch_size
					);
					tracing::error!("{message}");
					let err = anyhow!(message);
					Err(err)?;
				}

				let mut futures = Vec::with_capacity(batch_size);
				let mut compute_chars = 0;

				for input in inputs {
					compute_chars += input.count_chars();

					let local_infer = self.infer.clone();
					let prompt_name = prompt_name.clone();
					futures.push(async move {
						let permit = local_infer.acquire_permit().await;
						local_infer
							.embed_all(
								input,
								truncate,
								truncate_direction.into(),
								prompt_name,
								permit,
							)
							.await
					})
				}
				let results = join_all(futures)
					.await
					.into_iter()
					.collect::<Result<Vec<AllEmbeddingsInferResponse>, TextEmbeddingsError>>()
					.map_err(|err| anyhow!(err))?;

				let mut embeddings = Vec::with_capacity(batch_size);
				for r in results {
					embeddings.push(r.results);
				}
				EmbedAllResult(embeddings)
			}
		};

		tracing::info!("Success");

		Ok(result)
	}
}

#[derive(Debug)]
pub(crate) enum Embedding {
	Float(Vec<f32>),
	Base64(String),
}

#[derive(Debug)]
pub(crate) struct EmbedResult(pub Vec<Vec<f32>>);

#[derive(Debug)]
pub(crate) struct EmbedSparseResult(pub Vec<Vec<SparseValue>>);

#[derive(Debug)]
pub(crate) struct EmbedAllResult(pub Vec<Vec<Vec<f32>>>);

#[derive(Debug)]
pub(crate) struct SparseValue {
	pub(crate) index: usize,
	pub(crate) value: f32,
}
