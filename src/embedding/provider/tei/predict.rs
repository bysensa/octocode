use std::{
	sync::Arc,
	time::{Duration, Instant},
};

use anyhow::anyhow;
use futures::future::join_all;
use serde::Serialize;
use tei_core::infer::Infer;
use tokio::sync::OwnedSemaphorePermit;

use super::types::ModelTypeWrapper;

use super::{
	types::{Info, Sequence, TruncationDirection},
	Tei,
};

impl Tei {
	pub(crate) async fn predict(
		&self,
		input: PredictInput,
		raw_scores: bool,
		truncate: Option<bool>,
		truncation_direction: Option<TruncationDirection>,
	) -> anyhow::Result<PredictResult> {
		// Closure for predict
		let predict_inner = move |inputs: Sequence,
		                          truncate: bool,
		                          infer: Arc<Infer>,
		                          info: Arc<Info>,
		                          permit: Option<OwnedSemaphorePermit>| async move {
			let permit = match permit {
				None => infer.acquire_permit().await,
				Some(permit) => permit,
			};

			let truncation_direction = truncation_direction.unwrap_or(TruncationDirection::Right);
			let response = infer
				.predict(
					inputs,
					truncate,
					truncation_direction.into(),
					raw_scores,
					permit,
				)
				.await
				.map_err(|err| anyhow!(err))?;

			let id2label = match &info.model_type {
				ModelTypeWrapper::Classifier(classifier) => &classifier.id2label,
				ModelTypeWrapper::Reranker(classifier) => &classifier.id2label,
				_ => panic!(),
			};

			let mut predictions = Vec::with_capacity(response.results.len());
			for (i, s) in response.results.into_iter().enumerate() {
				// Check that s is not NaN or the partial_cmp below will panic
				if s.is_nan() {
					return Err(anyhow!("score is NaN"));
				}
				// Map score to label
				predictions.push(Prediction {
					score: s,
					label: id2label.get(&i.to_string()).unwrap().clone(),
				});
			}
			// Reverse sort
			predictions.sort_by(|x, y| x.score.partial_cmp(&y.score).unwrap());
			predictions.reverse();

			Ok::<Vec<Prediction>, anyhow::Error>(predictions)
		};

		let truncate = truncate.unwrap_or(self.info.auto_truncate);

		let res = match input {
			PredictInput::Single(inputs) => {
				let permit = self
					.infer
					.try_acquire_permit()
					.map_err(|err| anyhow!(err))?;
				let predictions = predict_inner(
					inputs,
					truncate,
					self.infer.clone(),
					self.info.clone(),
					Some(permit),
				)
				.await?;

				PredictResult::Single(predictions)
			}
			PredictInput::Batch(inputs) => {
				let batch_size = inputs.len();
				if batch_size > self.info.max_client_batch_size {
					let message = format!(
						"batch size {batch_size} > maximum allowed batch size {}",
						self.info.max_client_batch_size
					);
					tracing::error!("{message}");
					let err = anyhow!("{}", message);

					Err(err)?;
				}

				let mut futures = Vec::with_capacity(batch_size);

				for input in inputs {
					let local_infer = self.infer.clone();
					let local_info = self.info.clone();
					futures.push(predict_inner(
						input,
						truncate,
						local_infer,
						local_info,
						None,
					))
				}
				let results = join_all(futures)
					.await
					.into_iter()
					.collect::<Result<Vec<Vec<Prediction>>, anyhow::Error>>()?;

				let mut predictions = Vec::with_capacity(batch_size);
				for r in results {
					predictions.push(r);
				}

				PredictResult::Batch(predictions)
			}
		};

		tracing::info!("Success");

		Ok(res)
	}
}

#[derive(Debug)]
pub(crate) enum PredictInput {
	Single(Sequence),
	Batch(Vec<Sequence>),
}

#[derive(Debug)]
pub(crate) struct Prediction {
	pub(crate) score: f32,
	pub(crate) label: String,
}

#[derive(Debug)]
pub(crate) enum PredictResult {
	Single(Vec<Prediction>),
	Batch(Vec<Vec<Prediction>>),
}
