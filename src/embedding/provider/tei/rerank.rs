use std::sync::Arc;

use anyhow::anyhow;
use futures::future::join_all;
use tei_core::infer::Infer;

use super::{
	types::{ModelTypeWrapper, TruncationDirection},
	Tei,
};

impl Tei {
	pub(crate) async fn rerank(
		&self,
		RerankInput { query, texts }: RerankInput,
		raw_scores: bool,
		return_text: bool,
		truncate: Option<bool>,
		truncation_direction: Option<TruncationDirection>,
	) -> anyhow::Result<RerankResult> {
		if texts.is_empty() {
			let message = "`texts` cannot be empty".to_string();
			tracing::error!("{message}");
			let err = anyhow!(message);
			Err(err)?;
		}

		match &self.info.model_type {
			ModelTypeWrapper::Reranker(_) => Ok(()),
			ModelTypeWrapper::Classifier(_) | ModelTypeWrapper::Embedding(_) => {
				let message = "model is not a re-ranker model".to_string();
				Err(anyhow!(message))
			}
		}
		.map_err(|err| {
			tracing::error!("{err}");
			anyhow!(err)
		})?;

		// Closure for rerank
		let rerank_inner = move |query: String, text: String, truncate: bool, infer: Arc<Infer>| async move {
			let permit = infer.acquire_permit().await;
			let truncation_direction = truncation_direction.unwrap_or(TruncationDirection::Right);
			let response = infer
				.predict(
					(query, text),
					truncate,
					truncation_direction.into(),
					raw_scores,
					permit,
				)
				.await
				.map_err(|err| anyhow!(err))?;

			let score = response.results[0];

			Ok::<f32, anyhow::Error>(score)
		};

		let truncate = truncate.unwrap_or(self.info.auto_truncate);

		let res = {
			let batch_size = texts.len();
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

			for text in &texts {
				let local_infer = self.infer.clone();
				futures.push(rerank_inner(
					query.clone(),
					text.clone(),
					truncate,
					local_infer,
				))
			}
			let results = join_all(futures)
				.await
				.into_iter()
				.collect::<Result<Vec<f32>, anyhow::Error>>()?;

			let mut ranks = Vec::with_capacity(batch_size);
			for (index, r) in results.into_iter().enumerate() {
				let text = if return_text {
					Some(texts[index].clone())
				} else {
					None
				};

				let score = r;
				// Check that s is not NaN or the partial_cmp below will panic
				if score.is_nan() {
					Err(anyhow!("score is NaN"))?;
				}

				ranks.push(Rank { index, text, score })
			}

			// Reverse sort
			ranks.sort_by(|x, y| x.score.partial_cmp(&y.score).unwrap());
			ranks.reverse();

			RerankResult(ranks)
		};

		tracing::info!("Success");

		Ok(res)
	}
}

#[derive(Debug)]
pub(crate) struct RerankInput {
	pub(crate) query: String,
	pub(crate) texts: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct Rank {
	pub(crate) index: usize,
	pub(crate) text: Option<String>,
	pub(crate) score: f32,
}

#[derive(Debug)]
pub(crate) struct RerankResult(pub(crate) Vec<Rank>);
