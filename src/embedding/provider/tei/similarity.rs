use super::{
	types::{Input, InputType, TruncationDirection},
	Tei,
};
use anyhow::anyhow;
use simsimd::SpatialSimilarity;

impl Tei {
	pub(crate) async fn similarity(
		&self,
		input: SimilarityInput,
		truncate: Option<bool>,
		truncation_direction: Option<TruncationDirection>,
		prompt_name: Option<String>,
	) -> anyhow::Result<SimilarityResult> {
		if input.sentences.is_empty() {
			let message = "`inputs.sentences` cannot be empty".to_string();
			tracing::error!("{message}");
			let err = anyhow!(message);
			Err(err)?;
		}
		// +1 because of the source sentence
		let batch_size = input.sentences.len() + 1;
		if batch_size > self.info.max_client_batch_size {
			let message = format!(
				"batch size {batch_size} > maximum allowed batch size {}",
				self.info.max_client_batch_size
			);
			tracing::error!("{message}");
			let err = anyhow!(message);
			Err(err)?;
		}

		// Convert request to embed request
		let mut inputs = Vec::with_capacity(input.sentences.len() + 1);
		inputs.push(InputType::String(input.source_sentence));
		for s in input.sentences {
			inputs.push(InputType::String(s));
		}

		let embeddings = self
			.embed(
				Input::Batch(inputs),
				false,
				truncate,
				truncation_direction,
				prompt_name,
			)
			.await?;
		let embeddings = embeddings.0;

		// Compute cosine
		let distances = (1..batch_size)
			.map(|i| 1.0 - f32::cosine(&embeddings[0], &embeddings[i]).unwrap() as f32)
			.collect();

		Ok(SimilarityResult(distances))
	}
}

#[derive(Debug)]
pub(crate) struct SimilarityInput {
	/// The string that you wish to compare the other strings with. This can be a phrase, sentence,
	/// or longer passage, depending on the model being used.
	pub(crate) source_sentence: String,
	/// A list of strings which will be compared against the source_sentence.
	pub(crate) sentences: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct SimilarityResult(pub(crate) Vec<f32>);
