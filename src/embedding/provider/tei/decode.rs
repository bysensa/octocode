use std::sync::Arc;

use anyhow::anyhow;
use futures::future::join_all;
use tei_core::infer::Infer;

use super::Tei;

impl Tei {
	pub(crate) async fn decode(
		&self,
		input: InputIds,
		skip_special_tokens: Option<bool>,
	) -> anyhow::Result<DecodeResult> {
		let skip_special_tokens = skip_special_tokens.unwrap_or(true);
		let decode_inner = move |ids: Vec<u32>, skip_special_tokens: bool, infer: Arc<Infer>| async move {
			let text = infer
				.decode(ids, skip_special_tokens)
				.await
				.map_err(|err| anyhow!(err))?;
			Ok::<String, anyhow::Error>(text)
		};

		let texts = match input {
			InputIds::Single(ids) => {
				vec![decode_inner(ids, skip_special_tokens, self.infer.clone()).await?]
			}
			InputIds::Batch(ids) => {
				if ids.is_empty() {
					let message = "`ids` cannot be empty".to_string();
					tracing::error!("{message}");
					let err = anyhow!(message);
					Err(err)?;
				}

				let batch_size = ids.len();
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
				for ids in ids {
					futures.push(decode_inner(ids, skip_special_tokens, self.infer.clone()));
				}

				join_all(futures)
					.await
					.into_iter()
					.collect::<Result<Vec<String>, anyhow::Error>>()?
			}
		};
		Ok(DecodeResult(texts))
	}
}

#[derive(Debug)]
pub(crate) enum InputIds {
	Single(Vec<u32>),
	Batch(Vec<Vec<u32>>),
}

#[derive(Debug)]
pub(crate) struct DecodeResult(pub(crate) Vec<String>);
