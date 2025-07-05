use std::sync::Arc;

use anyhow::anyhow;
use futures::future::join_all;
use tei_core::{
	infer::Infer,
	tokenization::{into_tokens, SimpleToken},
};

use super::Tei;

impl Tei {
	pub(crate) async fn tokenize(
		&self,
		input: TokenizeInput,
		add_special_tokens: Option<bool>,
		prompt_name: Option<String>,
	) -> anyhow::Result<TokenizeResult> {
		let add_special_tokens = add_special_tokens.unwrap_or(true);
		let tokenize_inner = move |input: String,
		                           add_special_tokens: bool,
		                           prompt_name: Option<String>,
		                           infer: Arc<Infer>| async move {
			let (encoded_input, encoding) = infer
				.tokenize(input.clone(), add_special_tokens, prompt_name)
				.await
				.map_err(|err| anyhow!(err))?;
			let input = encoded_input.unwrap_or(input);

			let tokens: Vec<SimpleToken> = into_tokens(encoding, &input).into_iter().collect();
			Ok::<Vec<SimpleToken>, anyhow::Error>(tokens)
		};

		let tokens = match input {
			TokenizeInput::Single(input) => {
				let res =
					tokenize_inner(input, add_special_tokens, prompt_name, self.infer.clone())
						.await;
				vec![res?]
			}
			TokenizeInput::Batch(inputs) => {
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
					futures.push(tokenize_inner(
						input,
						add_special_tokens,
						prompt_name.clone(),
						self.infer.clone(),
					));
				}

				join_all(futures)
					.await
					.into_iter()
					.collect::<Result<Vec<Vec<SimpleToken>>, anyhow::Error>>()?
			}
		};
		Ok(TokenizeResult(tokens))
	}
}

#[derive(Debug)]
pub(crate) enum TokenizeInput {
	Single(String),
	Batch(Vec<String>),
}

#[derive(Debug)]
pub(crate) struct TokenizeResult(pub(crate) Vec<Vec<SimpleToken>>);
