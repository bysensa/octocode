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

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// Arrow imports
use arrow::array::{Array, FixedSizeListArray, Float32Array, Int64Array, StringArray, UInt32Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;

// LanceDB imports
use futures::TryStreamExt;
use lancedb::{
	connect,
	index::Index,
	query::{ExecutableQuery, QueryBase, Select},
	Connection, DistanceType,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodeBlock {
	pub path: String,
	pub language: String,
	pub content: String,
	pub symbols: Vec<String>,
	pub start_line: usize,
	pub end_line: usize,
	pub hash: String,
	// Optional distance field for relevance sorting (higher is more relevant)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub distance: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TextBlock {
	pub path: String,
	pub language: String,
	pub content: String,
	pub start_line: usize,
	pub end_line: usize,
	pub hash: String,
	// Optional distance field for relevance sorting (higher is more relevant)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub distance: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DocumentBlock {
	pub path: String,
	pub title: String,
	pub content: String,
	pub level: usize,
	pub start_line: usize,
	pub end_line: usize,
	pub hash: String,
	// Optional distance field for relevance sorting (higher is more relevant)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub distance: Option<f32>,
}

pub struct Store {
	db: Connection,
	code_vector_dim: usize, // Size of code embedding vectors
	text_vector_dim: usize, // Size of text embedding vectors
}

// Helper struct for converting between Arrow RecordBatch and our domain models
struct BatchConverter {
	vector_dim: usize,
}

impl BatchConverter {
	fn new(vector_dim: usize) -> Self {
		Self { vector_dim }
	}

	// Convert a CodeBlock to a RecordBatch
	fn code_block_to_batch(
		&self,
		blocks: &[CodeBlock],
		embeddings: &[Vec<f32>],
	) -> Result<RecordBatch> {
		// Ensure we have the same number of blocks and embeddings
		if blocks.len() != embeddings.len() {
			return Err(anyhow::anyhow!(
				"Number of blocks and embeddings must match"
			));
		}

		if blocks.is_empty() {
			return Err(anyhow::anyhow!("Empty blocks array"));
		}

		// Check if all embedding vectors have the expected dimension
		for (i, embedding) in embeddings.iter().enumerate() {
			if embedding.len() != self.vector_dim {
				return Err(anyhow::anyhow!(
					"Embedding at index {} has dimension {} but expected {}",
					i,
					embedding.len(),
					self.vector_dim
				));
			}
		}

		// Create schema
		let schema = Arc::new(Schema::new(vec![
			Field::new("id", DataType::Utf8, false),
			Field::new("path", DataType::Utf8, false),
			Field::new("language", DataType::Utf8, false),
			Field::new("content", DataType::Utf8, false),
			Field::new("symbols", DataType::Utf8, true), // Storing serialized JSON of symbols
			Field::new("start_line", DataType::UInt32, false),
			Field::new("end_line", DataType::UInt32, false),
			Field::new("hash", DataType::Utf8, false),
			Field::new(
				"embedding",
				DataType::FixedSizeList(
					Arc::new(Field::new("item", DataType::Float32, true)),
					self.vector_dim as i32,
				),
				true,
			),
		]));

		// Create arrays
		let ids: Vec<String> = blocks.iter().map(|_| Uuid::new_v4().to_string()).collect();
		let paths: Vec<&str> = blocks.iter().map(|b| b.path.as_str()).collect();
		let languages: Vec<&str> = blocks.iter().map(|b| b.language.as_str()).collect();
		let contents: Vec<&str> = blocks.iter().map(|b| b.content.as_str()).collect();
		let symbols: Vec<String> = blocks
			.iter()
			.map(|b| serde_json::to_string(&b.symbols).unwrap_or_default())
			.collect();
		let start_lines: Vec<u32> = blocks.iter().map(|b| b.start_line as u32).collect();
		let end_lines: Vec<u32> = blocks.iter().map(|b| b.end_line as u32).collect();
		let hashes: Vec<&str> = blocks.iter().map(|b| b.hash.as_str()).collect();

		// Create the embedding fixed size list array
		let mut flattened_embeddings = Vec::with_capacity(blocks.len() * self.vector_dim);
		for embedding in embeddings {
			flattened_embeddings.extend_from_slice(embedding);
		}
		let values = Float32Array::from(flattened_embeddings);

		// Create the fixed size list array
		let embedding_array = FixedSizeListArray::new(
			Arc::new(Field::new("item", DataType::Float32, true)),
			self.vector_dim as i32,
			Arc::new(values),
			None, // No validity buffer - all values are valid
		);

		// Verify all arrays have the same length
		let expected_len = blocks.len();
		assert_eq!(ids.len(), expected_len, "ids array length mismatch");
		assert_eq!(paths.len(), expected_len, "paths array length mismatch");
		assert_eq!(
			languages.len(),
			expected_len,
			"languages array length mismatch"
		);
		assert_eq!(
			contents.len(),
			expected_len,
			"contents array length mismatch"
		);
		assert_eq!(symbols.len(), expected_len, "symbols array length mismatch");
		assert_eq!(
			start_lines.len(),
			expected_len,
			"start_lines array length mismatch"
		);
		assert_eq!(
			end_lines.len(),
			expected_len,
			"end_lines array length mismatch"
		);
		assert_eq!(hashes.len(), expected_len, "hashes array length mismatch");
		assert_eq!(
			embedding_array.len(),
			expected_len,
			"embedding_array length mismatch"
		);

		// Create record batch
		let batch = RecordBatch::try_new(
			schema,
			vec![
				Arc::new(StringArray::from(ids)),
				Arc::new(StringArray::from(paths)),
				Arc::new(StringArray::from(languages)),
				Arc::new(StringArray::from(contents)),
				Arc::new(StringArray::from(symbols)),
				Arc::new(UInt32Array::from(start_lines)),
				Arc::new(UInt32Array::from(end_lines)),
				Arc::new(StringArray::from(hashes)),
				Arc::new(embedding_array),
			],
		)?;

		Ok(batch)
	}

	// Convert a TextBlock to a RecordBatch
	fn text_block_to_batch(
		&self,
		blocks: &[TextBlock],
		embeddings: &[Vec<f32>],
	) -> Result<RecordBatch> {
		// Ensure we have the same number of blocks and embeddings
		if blocks.len() != embeddings.len() {
			return Err(anyhow::anyhow!(
				"Number of blocks and embeddings must match"
			));
		}

		if blocks.is_empty() {
			return Err(anyhow::anyhow!("Empty blocks array"));
		}

		// Check if all embedding vectors have the expected dimension
		for (i, embedding) in embeddings.iter().enumerate() {
			if embedding.len() != self.vector_dim {
				return Err(anyhow::anyhow!(
					"Embedding at index {} has dimension {} but expected {}",
					i,
					embedding.len(),
					self.vector_dim
				));
			}
		}

		// Create schema
		let schema = Arc::new(Schema::new(vec![
			Field::new("id", DataType::Utf8, false),
			Field::new("path", DataType::Utf8, false),
			Field::new("language", DataType::Utf8, false),
			Field::new("content", DataType::Utf8, false),
			Field::new("start_line", DataType::UInt32, false),
			Field::new("end_line", DataType::UInt32, false),
			Field::new("hash", DataType::Utf8, false),
			Field::new(
				"embedding",
				DataType::FixedSizeList(
					Arc::new(Field::new("item", DataType::Float32, true)),
					self.vector_dim as i32,
				),
				true,
			),
		]));

		// Create arrays
		let ids: Vec<String> = blocks.iter().map(|_| Uuid::new_v4().to_string()).collect();
		let paths: Vec<&str> = blocks.iter().map(|b| b.path.as_str()).collect();
		let languages: Vec<&str> = blocks.iter().map(|b| b.language.as_str()).collect();
		let contents: Vec<&str> = blocks.iter().map(|b| b.content.as_str()).collect();
		let start_lines: Vec<u32> = blocks.iter().map(|b| b.start_line as u32).collect();
		let end_lines: Vec<u32> = blocks.iter().map(|b| b.end_line as u32).collect();
		let hashes: Vec<&str> = blocks.iter().map(|b| b.hash.as_str()).collect();

		// Create the embedding fixed size list array
		let mut flattened_embeddings = Vec::with_capacity(blocks.len() * self.vector_dim);
		for embedding in embeddings {
			flattened_embeddings.extend_from_slice(embedding);
		}
		let values = Float32Array::from(flattened_embeddings);

		// Create the fixed size list array
		let embedding_array = FixedSizeListArray::new(
			Arc::new(Field::new("item", DataType::Float32, true)),
			self.vector_dim as i32,
			Arc::new(values),
			None, // No validity buffer - all values are valid
		);

		// Verify all arrays have the same length
		let expected_len = blocks.len();
		assert_eq!(ids.len(), expected_len, "ids array length mismatch");
		assert_eq!(paths.len(), expected_len, "paths array length mismatch");
		assert_eq!(
			languages.len(),
			expected_len,
			"languages array length mismatch"
		);
		assert_eq!(
			contents.len(),
			expected_len,
			"contents array length mismatch"
		);
		assert_eq!(
			start_lines.len(),
			expected_len,
			"start_lines array length mismatch"
		);
		assert_eq!(
			end_lines.len(),
			expected_len,
			"end_lines array length mismatch"
		);
		assert_eq!(hashes.len(), expected_len, "hashes array length mismatch");
		assert_eq!(
			embedding_array.len(),
			expected_len,
			"embedding_array length mismatch"
		);

		// Create record batch
		let batch = RecordBatch::try_new(
			schema,
			vec![
				Arc::new(StringArray::from(ids)),
				Arc::new(StringArray::from(paths)),
				Arc::new(StringArray::from(languages)),
				Arc::new(StringArray::from(contents)),
				Arc::new(UInt32Array::from(start_lines)),
				Arc::new(UInt32Array::from(end_lines)),
				Arc::new(StringArray::from(hashes)),
				Arc::new(embedding_array),
			],
		)?;

		Ok(batch)
	}

	// Convert a DocumentBlock to a RecordBatch
	fn document_block_to_batch(
		&self,
		blocks: &[DocumentBlock],
		embeddings: &[Vec<f32>],
	) -> Result<RecordBatch> {
		// Ensure we have the same number of blocks and embeddings
		if blocks.len() != embeddings.len() {
			return Err(anyhow::anyhow!(
				"Number of blocks and embeddings must match"
			));
		}

		if blocks.is_empty() {
			return Err(anyhow::anyhow!("Empty blocks array"));
		}

		// Check if all embedding vectors have the expected dimension
		for (i, embedding) in embeddings.iter().enumerate() {
			if embedding.len() != self.vector_dim {
				return Err(anyhow::anyhow!(
					"Embedding at index {} has dimension {} but expected {}",
					i,
					embedding.len(),
					self.vector_dim
				));
			}
		}

		// Create schema
		let schema = Arc::new(Schema::new(vec![
			Field::new("id", DataType::Utf8, false),
			Field::new("path", DataType::Utf8, false),
			Field::new("title", DataType::Utf8, false),
			Field::new("content", DataType::Utf8, false),
			Field::new("level", DataType::UInt32, false),
			Field::new("start_line", DataType::UInt32, false),
			Field::new("end_line", DataType::UInt32, false),
			Field::new("hash", DataType::Utf8, false),
			Field::new(
				"embedding",
				DataType::FixedSizeList(
					Arc::new(Field::new("item", DataType::Float32, true)),
					self.vector_dim as i32,
				),
				true,
			),
		]));

		// Create arrays
		let ids: Vec<String> = blocks.iter().map(|_| Uuid::new_v4().to_string()).collect();
		let paths: Vec<&str> = blocks.iter().map(|b| b.path.as_str()).collect();
		let titles: Vec<&str> = blocks.iter().map(|b| b.title.as_str()).collect();
		let contents: Vec<&str> = blocks.iter().map(|b| b.content.as_str()).collect();
		let levels: Vec<u32> = blocks.iter().map(|b| b.level as u32).collect();
		let start_lines: Vec<u32> = blocks.iter().map(|b| b.start_line as u32).collect();
		let end_lines: Vec<u32> = blocks.iter().map(|b| b.end_line as u32).collect();
		let hashes: Vec<&str> = blocks.iter().map(|b| b.hash.as_str()).collect();

		// Create the embedding fixed size list array
		let mut flattened_embeddings = Vec::with_capacity(blocks.len() * self.vector_dim);
		for embedding in embeddings {
			flattened_embeddings.extend_from_slice(embedding);
		}
		let values = Float32Array::from(flattened_embeddings);

		// Create the fixed size list array
		let embedding_array = FixedSizeListArray::new(
			Arc::new(Field::new("item", DataType::Float32, true)),
			self.vector_dim as i32,
			Arc::new(values),
			None, // No validity buffer - all values are valid
		);

		// Verify all arrays have the same length
		let expected_len = blocks.len();
		assert_eq!(ids.len(), expected_len, "ids array length mismatch");
		assert_eq!(paths.len(), expected_len, "paths array length mismatch");
		assert_eq!(titles.len(), expected_len, "titles array length mismatch");
		assert_eq!(
			contents.len(),
			expected_len,
			"contents array length mismatch"
		);
		assert_eq!(levels.len(), expected_len, "levels array length mismatch");
		assert_eq!(
			start_lines.len(),
			expected_len,
			"start_lines array length mismatch"
		);
		assert_eq!(
			end_lines.len(),
			expected_len,
			"end_lines array length mismatch"
		);
		assert_eq!(hashes.len(), expected_len, "hashes array length mismatch");
		assert_eq!(
			embedding_array.len(),
			expected_len,
			"embedding_array length mismatch"
		);

		// Create record batch
		let batch = RecordBatch::try_new(
			schema,
			vec![
				Arc::new(StringArray::from(ids)),
				Arc::new(StringArray::from(paths)),
				Arc::new(StringArray::from(titles)),
				Arc::new(StringArray::from(contents)),
				Arc::new(UInt32Array::from(levels)),
				Arc::new(UInt32Array::from(start_lines)),
				Arc::new(UInt32Array::from(end_lines)),
				Arc::new(StringArray::from(hashes)),
				Arc::new(embedding_array),
			],
		)?;

		Ok(batch)
	}

	// Convert a RecordBatch to a Vec of CodeBlocks
	fn batch_to_code_blocks(
		&self,
		batch: &RecordBatch,
		distances: Option<Vec<f32>>,
	) -> Result<Vec<CodeBlock>> {
		let num_rows = batch.num_rows();
		let mut code_blocks = Vec::with_capacity(num_rows);

		let path_array = batch
			.column_by_name("path")
			.ok_or_else(|| anyhow::anyhow!("'path' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'path' column is not a StringArray"))?;

		let language_array = batch
			.column_by_name("language")
			.ok_or_else(|| anyhow::anyhow!("'language' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'language' column is not a StringArray"))?;

		let content_array = batch
			.column_by_name("content")
			.ok_or_else(|| anyhow::anyhow!("'content' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'content' column is not a StringArray"))?;

		let symbols_array = batch
			.column_by_name("symbols")
			.ok_or_else(|| anyhow::anyhow!("'symbols' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'symbols' column is not a StringArray"))?;

		let start_line_array = batch
			.column_by_name("start_line")
			.ok_or_else(|| anyhow::anyhow!("'start_line' column not found"))?
			.as_any()
			.downcast_ref::<UInt32Array>()
			.ok_or_else(|| anyhow::anyhow!("'start_line' column is not a UInt32Array"))?;

		let end_line_array = batch
			.column_by_name("end_line")
			.ok_or_else(|| anyhow::anyhow!("'end_line' column not found"))?
			.as_any()
			.downcast_ref::<UInt32Array>()
			.ok_or_else(|| anyhow::anyhow!("'end_line' column is not a UInt32Array"))?;

		let hash_array = batch
			.column_by_name("hash")
			.ok_or_else(|| anyhow::anyhow!("'hash' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'hash' column is not a StringArray"))?;

		for i in 0..num_rows {
			let symbols: Vec<String> = if symbols_array.is_null(i) {
				Vec::new()
			} else {
				serde_json::from_str::<Vec<String>>(symbols_array.value(i)).unwrap_or_default()
			};

			let distance = distances.as_ref().map(|d| d[i]);

			code_blocks.push(CodeBlock {
				path: path_array.value(i).to_string(),
				language: language_array.value(i).to_string(),
				content: content_array.value(i).to_string(),
				symbols,
				start_line: start_line_array.value(i) as usize,
				end_line: end_line_array.value(i) as usize,
				hash: hash_array.value(i).to_string(),
				distance,
			});
		}

		Ok(code_blocks)
	}

	// Convert a RecordBatch to a Vec of TextBlocks
	fn batch_to_text_blocks(
		&self,
		batch: &RecordBatch,
		distances: Option<Vec<f32>>,
	) -> Result<Vec<TextBlock>> {
		let num_rows = batch.num_rows();
		let mut text_blocks = Vec::with_capacity(num_rows);

		let path_array = batch
			.column_by_name("path")
			.ok_or_else(|| anyhow::anyhow!("'path' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'path' column is not a StringArray"))?;

		let language_array = batch
			.column_by_name("language")
			.ok_or_else(|| anyhow::anyhow!("'language' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'language' column is not a StringArray"))?;

		let content_array = batch
			.column_by_name("content")
			.ok_or_else(|| anyhow::anyhow!("'content' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'content' column is not a StringArray"))?;

		let start_line_array = batch
			.column_by_name("start_line")
			.ok_or_else(|| anyhow::anyhow!("'start_line' column not found"))?
			.as_any()
			.downcast_ref::<UInt32Array>()
			.ok_or_else(|| anyhow::anyhow!("'start_line' column is not a UInt32Array"))?;

		let end_line_array = batch
			.column_by_name("end_line")
			.ok_or_else(|| anyhow::anyhow!("'end_line' column not found"))?
			.as_any()
			.downcast_ref::<UInt32Array>()
			.ok_or_else(|| anyhow::anyhow!("'end_line' column is not a UInt32Array"))?;

		let hash_array = batch
			.column_by_name("hash")
			.ok_or_else(|| anyhow::anyhow!("'hash' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'hash' column is not a StringArray"))?;

		for i in 0..num_rows {
			let distance = distances.as_ref().map(|d| d[i]);

			text_blocks.push(TextBlock {
				path: path_array.value(i).to_string(),
				language: language_array.value(i).to_string(),
				content: content_array.value(i).to_string(),
				start_line: start_line_array.value(i) as usize,
				end_line: end_line_array.value(i) as usize,
				hash: hash_array.value(i).to_string(),
				distance,
			});
		}

		Ok(text_blocks)
	}

	// Convert a RecordBatch to a Vec of DocumentBlocks
	fn batch_to_document_blocks(
		&self,
		batch: &RecordBatch,
		distances: Option<Vec<f32>>,
	) -> Result<Vec<DocumentBlock>> {
		let num_rows = batch.num_rows();
		let mut document_blocks = Vec::with_capacity(num_rows);

		let path_array = batch
			.column_by_name("path")
			.ok_or_else(|| anyhow::anyhow!("'path' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'path' column is not a StringArray"))?;

		let title_array = batch
			.column_by_name("title")
			.ok_or_else(|| anyhow::anyhow!("'title' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'title' column is not a StringArray"))?;

		let content_array = batch
			.column_by_name("content")
			.ok_or_else(|| anyhow::anyhow!("'content' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'content' column is not a StringArray"))?;

		let level_array = batch
			.column_by_name("level")
			.ok_or_else(|| anyhow::anyhow!("'level' column not found"))?
			.as_any()
			.downcast_ref::<UInt32Array>()
			.ok_or_else(|| anyhow::anyhow!("'level' column is not a UInt32Array"))?;

		let start_line_array = batch
			.column_by_name("start_line")
			.ok_or_else(|| anyhow::anyhow!("'start_line' column not found"))?
			.as_any()
			.downcast_ref::<UInt32Array>()
			.ok_or_else(|| anyhow::anyhow!("'start_line' column is not a UInt32Array"))?;

		let end_line_array = batch
			.column_by_name("end_line")
			.ok_or_else(|| anyhow::anyhow!("'end_line' column not found"))?
			.as_any()
			.downcast_ref::<UInt32Array>()
			.ok_or_else(|| anyhow::anyhow!("'end_line' column is not a UInt32Array"))?;

		let hash_array = batch
			.column_by_name("hash")
			.ok_or_else(|| anyhow::anyhow!("'hash' column not found"))?
			.as_any()
			.downcast_ref::<StringArray>()
			.ok_or_else(|| anyhow::anyhow!("'hash' column is not a StringArray"))?;

		for i in 0..num_rows {
			let distance = distances.as_ref().map(|d| d[i]);

			document_blocks.push(DocumentBlock {
				path: path_array.value(i).to_string(),
				title: title_array.value(i).to_string(),
				content: content_array.value(i).to_string(),
				level: level_array.value(i) as usize,
				start_line: start_line_array.value(i) as usize,
				end_line: end_line_array.value(i) as usize,
				hash: hash_array.value(i).to_string(),
				distance,
			});
		}

		Ok(document_blocks)
	}
}

// Implementing Drop for the Store
impl Drop for Store {
	fn drop(&mut self) {
		if cfg!(debug_assertions) {
			println!("Store instance dropped, database connection released");
		}
	}
}

impl Store {
	pub async fn new() -> Result<Self> {
		// Get current directory
		let current_dir = std::env::current_dir()?;

		// Get the project database path using the new storage system
		let index_path = crate::storage::get_project_database_path(&current_dir)?;

		// Ensure the directory exists
		crate::storage::ensure_project_storage_exists(&current_dir)?;

		// Ensure the database directory exists
		if !index_path.exists() {
			std::fs::create_dir_all(&index_path)?;
		}

		// Convert the path to a string for the file-based database
		let storage_path = index_path
			.to_str()
			.ok_or_else(|| anyhow::anyhow!("Invalid database path"))?;

		// Load the config to get the embedding provider and model info
		let config = crate::config::Config::load()?;

		// Get vector dimensions from both code and text model configurations
		let (code_provider, code_model) =
			crate::embedding::parse_provider_model(&config.embedding.code_model);
		let code_vector_dim = config
			.embedding
			.get_vector_dimension(&code_provider, &code_model);

		let (text_provider, text_model) =
			crate::embedding::parse_provider_model(&config.embedding.text_model);
		let text_vector_dim = config
			.embedding
			.get_vector_dimension(&text_provider, &text_model);

		// Connect to LanceDB
		let db = connect(storage_path).execute().await?;

		// Check if tables exist and if their schema matches the current configuration
		let table_names = db.table_names().execute().await?;

		// Check for schema mismatches and recreate tables if necessary
		for table_name in [
			"code_blocks",
			"text_blocks",
			"document_blocks",
			"graphrag_nodes",
		] {
			if table_names.contains(&table_name.to_string()) {
				if let Ok(table) = db.open_table(table_name).execute().await {
					if let Ok(schema) = table.schema().await {
						// Check if embedding field has the right dimension
						if let Ok(field) = schema.field_with_name("embedding") {
							if let DataType::FixedSizeList(_, size) = field.data_type() {
								let expected_dim = match table_name {
									"code_blocks" | "graphrag_nodes" => code_vector_dim as i32,
									"text_blocks" | "document_blocks" => text_vector_dim as i32,
									_ => continue,
								};

								if size != &expected_dim {
									println!("Schema mismatch detected for table '{}': expected dimension {}, found {}. Dropping table for recreation.",
										table_name, expected_dim, size);
									drop(table); // Release table handle before dropping
									if let Err(e) = db.drop_table(table_name).await {
										eprintln!(
											"Warning: Failed to drop table {}: {}",
											table_name, e
										);
									}
								}
							}
						}
					}
				}
			}
		}

		Ok(Self {
			db,
			code_vector_dim,
			text_vector_dim,
		})
	}

	pub async fn initialize_collections(&self) -> Result<()> {
		// Check if tables exist, if not create them
		let table_names = self.db.table_names().execute().await?;

		// Create code_blocks table if it doesn't exist
		if !table_names.contains(&"code_blocks".to_string()) {
			// Create empty table with schema
			let schema = Arc::new(Schema::new(vec![
				Field::new("id", DataType::Utf8, false),
				Field::new("path", DataType::Utf8, false),
				Field::new("language", DataType::Utf8, false),
				Field::new("content", DataType::Utf8, false),
				Field::new("symbols", DataType::Utf8, true),
				Field::new("start_line", DataType::UInt32, false),
				Field::new("end_line", DataType::UInt32, false),
				Field::new("hash", DataType::Utf8, false),
				Field::new(
					"embedding",
					DataType::FixedSizeList(
						Arc::new(Field::new("item", DataType::Float32, true)),
						self.code_vector_dim as i32,
					),
					true,
				),
			]));

			let _table = self
				.db
				.create_empty_table("code_blocks", schema)
				.execute()
				.await?;

			// Note: We'll create the index later when we have data
		}

		// Create text_blocks table if it doesn't exist
		if !table_names.contains(&"text_blocks".to_string()) {
			// Create empty table with schema
			let schema = Arc::new(Schema::new(vec![
				Field::new("id", DataType::Utf8, false),
				Field::new("path", DataType::Utf8, false),
				Field::new("language", DataType::Utf8, false),
				Field::new("content", DataType::Utf8, false),
				Field::new("start_line", DataType::UInt32, false),
				Field::new("end_line", DataType::UInt32, false),
				Field::new("hash", DataType::Utf8, false),
				Field::new(
					"embedding",
					DataType::FixedSizeList(
						Arc::new(Field::new("item", DataType::Float32, true)),
						self.text_vector_dim as i32,
					),
					true,
				),
			]));

			let _table = self
				.db
				.create_empty_table("text_blocks", schema)
				.execute()
				.await?;

			// Note: We'll create the index later when we have data
		}

		// Create document_blocks table if it doesn't exist
		if !table_names.contains(&"document_blocks".to_string()) {
			// Create empty table with schema
			let schema = Arc::new(Schema::new(vec![
				Field::new("id", DataType::Utf8, false),
				Field::new("path", DataType::Utf8, false),
				Field::new("title", DataType::Utf8, false),
				Field::new("content", DataType::Utf8, false),
				Field::new("level", DataType::UInt32, false),
				Field::new("start_line", DataType::UInt32, false),
				Field::new("end_line", DataType::UInt32, false),
				Field::new("hash", DataType::Utf8, false),
				Field::new(
					"embedding",
					DataType::FixedSizeList(
						Arc::new(Field::new("item", DataType::Float32, true)),
						self.text_vector_dim as i32,
					),
					true,
				),
			]));

			let _table = self
				.db
				.create_empty_table("document_blocks", schema)
				.execute()
				.await?;

			// Note: We'll create the index later when we have data
		}

		// Create graphrag_nodes table if it doesn't exist
		if !table_names.contains(&"graphrag_nodes".to_string()) {
			// Create empty table with schema
			let schema = Arc::new(Schema::new(vec![
				Field::new("id", DataType::Utf8, false),
				Field::new("name", DataType::Utf8, false),
				Field::new("kind", DataType::Utf8, false),
				Field::new("path", DataType::Utf8, false),
				Field::new("description", DataType::Utf8, false),
				Field::new("symbols", DataType::Utf8, true), // JSON serialized
				Field::new("hash", DataType::Utf8, false),
				Field::new(
					"embedding",
					DataType::FixedSizeList(
						Arc::new(Field::new("item", DataType::Float32, true)),
						self.code_vector_dim as i32,
					),
					true,
				),
			]));

			let _table = self
				.db
				.create_empty_table("graphrag_nodes", schema)
				.execute()
				.await?;
		}

		// Create graphrag_relationships table if it doesn't exist
		if !table_names.contains(&"graphrag_relationships".to_string()) {
			// Create empty table with schema
			let schema = Arc::new(Schema::new(vec![
				Field::new("id", DataType::Utf8, false),
				Field::new("source", DataType::Utf8, false),
				Field::new("target", DataType::Utf8, false),
				Field::new("relation_type", DataType::Utf8, false),
				Field::new("description", DataType::Utf8, false),
				Field::new("confidence", DataType::Float32, false),
			]));

			let _table = self
				.db
				.create_empty_table("graphrag_relationships", schema)
				.execute()
				.await?;
		}

		Ok(())
	}

	pub async fn content_exists(&self, hash: &str, collection: &str) -> Result<bool> {
		let table = self.db.open_table(collection).execute().await?;

		// Query to check if a record with the given hash exists
		let mut results = table
			.query()
			.only_if(format!("hash = '{}'", hash))
			.limit(1)
			.execute()
			.await?;

		// Check if any batch contains rows
		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() > 0 {
				return Ok(true);
			}
		}
		Ok(false)
	}

	pub async fn store_code_blocks(
		&self,
		blocks: &[CodeBlock],
		embeddings: Vec<Vec<f32>>,
	) -> Result<()> {
		if blocks.is_empty() {
			return Ok(());
		}

		// Check for dimension mismatches and handle them
		for (i, embedding) in embeddings.iter().enumerate() {
			if embedding.len() != self.code_vector_dim {
				return Err(anyhow::anyhow!(
					"Code embedding at index {} has dimension {} but expected {}",
					i,
					embedding.len(),
					self.code_vector_dim
				));
			}
		}

		// Convert blocks to RecordBatch
		let converter = BatchConverter::new(self.code_vector_dim);
		let batch = converter.code_block_to_batch(blocks, &embeddings)?;

		// Open or create the table
		let table = self.db.open_table("code_blocks").execute().await?;

		// Create an iterator that yields this single batch
		use std::iter::once;
		let batch_clone = batch.clone();
		let schema = batch_clone.schema();
		let batches = once(Ok(batch));
		let batch_reader = arrow::record_batch::RecordBatchIterator::new(batches, schema);

		// Add the batch to the table
		table.add(batch_reader).execute().await?;

		// Check if index exists and create it if needed
		let has_index = table
			.list_indices()
			.await?
			.iter()
			.any(|idx| idx.columns == vec!["embedding"]);
		let row_count = table.count_rows(None).await?;
		if !has_index && row_count > 256 {
			table
				.create_index(&["embedding"], Index::Auto)
				.execute()
				.await?
		}

		Ok(())
	}

	pub async fn store_text_blocks(
		&self,
		blocks: &[TextBlock],
		embeddings: Vec<Vec<f32>>,
	) -> Result<()> {
		if blocks.is_empty() {
			return Ok(());
		}

		// Check for dimension mismatches and handle them
		for (i, embedding) in embeddings.iter().enumerate() {
			if embedding.len() != self.text_vector_dim {
				return Err(anyhow::anyhow!(
					"Text embedding at index {} has dimension {} but expected {}",
					i,
					embedding.len(),
					self.text_vector_dim
				));
			}
		}

		// Convert blocks to RecordBatch
		let converter = BatchConverter::new(self.text_vector_dim);
		let batch = converter.text_block_to_batch(blocks, &embeddings)?;

		// Open or create the table
		let table = self.db.open_table("text_blocks").execute().await?;

		// Create an iterator that yields this single batch
		use std::iter::once;
		let batch_clone = batch.clone();
		let schema = batch_clone.schema();
		let batches = once(Ok(batch));
		let batch_reader = arrow::record_batch::RecordBatchIterator::new(batches, schema);

		// Add the batch to the table
		table.add(batch_reader).execute().await?;

		// Check if index exists and create it if needed
		let has_index = table
			.list_indices()
			.await?
			.iter()
			.any(|idx| idx.columns == vec!["embedding"]);
		let row_count = table.count_rows(None).await?;
		if !has_index && row_count > 256 {
			table
				.create_index(&["embedding"], Index::Auto)
				.execute()
				.await?
		}

		Ok(())
	}

	pub async fn store_document_blocks(
		&self,
		blocks: &[DocumentBlock],
		embeddings: Vec<Vec<f32>>,
	) -> Result<()> {
		if blocks.is_empty() {
			return Ok(());
		}

		// Check for dimension mismatches and handle them
		for (i, embedding) in embeddings.iter().enumerate() {
			if embedding.len() != self.text_vector_dim {
				return Err(anyhow::anyhow!(
					"Document embedding at index {} has dimension {} but expected {}",
					i,
					embedding.len(),
					self.text_vector_dim
				));
			}
		}

		// Convert blocks to RecordBatch
		let converter = BatchConverter::new(self.text_vector_dim);
		let batch = converter.document_block_to_batch(blocks, &embeddings)?;

		// Open or create the table
		let table = self.db.open_table("document_blocks").execute().await?;

		// Create an iterator that yields this single batch
		use std::iter::once;
		let batch_clone = batch.clone();
		let schema = batch_clone.schema();
		let batches = once(Ok(batch));
		let batch_reader = arrow::record_batch::RecordBatchIterator::new(batches, schema);

		// Add the batch to the table
		table.add(batch_reader).execute().await?;

		// Check if index exists and create it if needed
		let has_index = table
			.list_indices()
			.await?
			.iter()
			.any(|idx| idx.columns == vec!["embedding"]);
		let row_count = table.count_rows(None).await?;
		if !has_index && row_count > 256 {
			table
				.create_index(&["embedding"], Index::Auto)
				.execute()
				.await?
		}

		Ok(())
	}

	pub async fn get_code_blocks(&self, embedding: Vec<f32>) -> Result<Vec<CodeBlock>> {
		self.get_code_blocks_with_config(embedding, None, None)
			.await
	}

	pub async fn get_code_blocks_with_config(
		&self,
		embedding: Vec<f32>,
		max_results: Option<usize>,
		similarity_threshold: Option<f32>,
	) -> Result<Vec<CodeBlock>> {
		// Check embedding dimension
		if embedding.len() != self.code_vector_dim {
			return Err(anyhow::anyhow!(
				"Search embedding has dimension {} but expected {}",
				embedding.len(),
				self.code_vector_dim
			));
		}

		// Open the table
		let table = self.db.open_table("code_blocks").execute().await?;

		// Check if the table has any data
		let row_count = table.count_rows(None).await?;
		if row_count == 0 {
			// No data, return empty vector
			return Ok(Vec::new());
		}

		// Check if index exists and create it if needed
		let has_index = table
			.list_indices()
			.await?
			.iter()
			.any(|idx| idx.columns == vec!["embedding"]);
		if !has_index && row_count > 256 {
			table
				.create_index(&["embedding"], Index::Auto)
				.execute()
				.await?
		}

		// Use provided max_results or default to a reasonable number
		let limit = max_results.unwrap_or(50);

		// Perform vector search
		let mut results = table
			.query()
			.nearest_to(embedding.as_slice())? // Pass as slice instead of reference to Vec
			.distance_type(DistanceType::Cosine) // Explicitly use cosine distance
			.limit(limit)
			.execute()
			.await?;

		let mut all_code_blocks = Vec::new();
		let mut all_distances = Vec::new();

		// Process all batches
		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() == 0 {
				continue;
			}

			// Extract _distance column which contains similarity scores
			let distance_column = batch
				.column_by_name("_distance")
				.ok_or_else(|| anyhow::anyhow!("Distance column not found"))?;

			let distance_array = distance_column
				.as_any()
				.downcast_ref::<Float32Array>()
				.ok_or_else(|| {
					anyhow::anyhow!("Could not downcast distance column to Float32Array")
				})?;

			// Convert distances to Vec<f32>
			let distances: Vec<f32> = (0..distance_array.len())
				.map(|i| distance_array.value(i))
				.collect();

			// Convert results to CodeBlock structs
			let converter = BatchConverter::new(self.code_vector_dim);
			let mut code_blocks =
				converter.batch_to_code_blocks(&batch, Some(distances.clone()))?;

			all_code_blocks.append(&mut code_blocks);
			all_distances.extend(distances);
		}

		if all_code_blocks.is_empty() {
			return Ok(Vec::new());
		}

		// Filter by similarity threshold if provided
		if let Some(threshold) = similarity_threshold {
			all_code_blocks.retain(|block| {
				if let Some(distance) = block.distance {
					distance <= threshold // Lower distance means higher similarity
				} else {
					true // Keep blocks without distance info
				}
			});
		}

		// Sort by relevance (distance) - lower distance means higher similarity, so sort ascending
		all_code_blocks.sort_by(|a, b| {
			match (a.distance, b.distance) {
				(Some(dist_a), Some(dist_b)) => dist_a
					.partial_cmp(&dist_b)
					.unwrap_or(std::cmp::Ordering::Equal),
				(Some(_), None) => std::cmp::Ordering::Less, // Items with distance come first
				(None, Some(_)) => std::cmp::Ordering::Greater, // Items without distance come last
				(None, None) => std::cmp::Ordering::Equal,   // Equal if both have no distance
			}
		});

		Ok(all_code_blocks)
	}

	pub async fn get_document_blocks(&self, embedding: Vec<f32>) -> Result<Vec<DocumentBlock>> {
		self.get_document_blocks_with_config(embedding, None, None)
			.await
	}

	pub async fn get_document_blocks_with_config(
		&self,
		embedding: Vec<f32>,
		max_results: Option<usize>,
		similarity_threshold: Option<f32>,
	) -> Result<Vec<DocumentBlock>> {
		// Check embedding dimension
		if embedding.len() != self.text_vector_dim {
			return Err(anyhow::anyhow!(
				"Search embedding has dimension {} but expected {}",
				embedding.len(),
				self.text_vector_dim
			));
		}

		// Open the table
		let table = self.db.open_table("document_blocks").execute().await?;

		// Check if the table has any data
		let row_count = table.count_rows(None).await?;
		if row_count == 0 {
			// No data, return empty vector
			return Ok(Vec::new());
		}

		// Check if index exists and create it if needed
		let has_index = table
			.list_indices()
			.await?
			.iter()
			.any(|idx| idx.columns == vec!["embedding"]);
		if !has_index && row_count > 256 {
			table
				.create_index(&["embedding"], Index::Auto)
				.execute()
				.await?
		}

		// Use provided max_results or default to a reasonable number
		let limit = max_results.unwrap_or(50);

		// Perform vector search
		let mut results = table
			.query()
			.nearest_to(embedding.as_slice())? // Pass as slice instead of reference to Vec
			.distance_type(DistanceType::Cosine) // Explicitly use cosine distance
			.limit(limit)
			.execute()
			.await?;

		let mut all_document_blocks = Vec::new();

		// Process all batches
		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() == 0 {
				continue;
			}

			// Extract _distance column which contains similarity scores
			let distance_column = batch
				.column_by_name("_distance")
				.ok_or_else(|| anyhow::anyhow!("Distance column not found"))?;

			let distance_array = distance_column
				.as_any()
				.downcast_ref::<Float32Array>()
				.ok_or_else(|| {
					anyhow::anyhow!("Could not downcast distance column to Float32Array")
				})?;

			// Convert distances to Vec<f32>
			let distances: Vec<f32> = (0..distance_array.len())
				.map(|i| distance_array.value(i))
				.collect();

			// Convert results to DocumentBlock structs
			let converter = BatchConverter::new(self.text_vector_dim);
			let mut document_blocks =
				converter.batch_to_document_blocks(&batch, Some(distances))?;

			all_document_blocks.append(&mut document_blocks);
		}

		if all_document_blocks.is_empty() {
			return Ok(Vec::new());
		}

		// Filter by similarity threshold if provided
		if let Some(threshold) = similarity_threshold {
			all_document_blocks.retain(|block| {
				if let Some(distance) = block.distance {
					distance <= threshold // Lower distance means higher similarity
				} else {
					true // Keep blocks without distance info
				}
			});
		}

		// Sort by relevance (distance) - lower distance means higher similarity, so sort ascending
		all_document_blocks.sort_by(|a, b| {
			match (a.distance, b.distance) {
				(Some(dist_a), Some(dist_b)) => dist_a
					.partial_cmp(&dist_b)
					.unwrap_or(std::cmp::Ordering::Equal),
				(Some(_), None) => std::cmp::Ordering::Less, // Items with distance come first
				(None, Some(_)) => std::cmp::Ordering::Greater, // Items without distance come last
				(None, None) => std::cmp::Ordering::Equal,   // Equal if both have no distance
			}
		});

		Ok(all_document_blocks)
	}

	pub async fn get_text_blocks(&self, embedding: Vec<f32>) -> Result<Vec<TextBlock>> {
		self.get_text_blocks_with_config(embedding, None, None)
			.await
	}

	pub async fn get_text_blocks_with_config(
		&self,
		embedding: Vec<f32>,
		max_results: Option<usize>,
		similarity_threshold: Option<f32>,
	) -> Result<Vec<TextBlock>> {
		// Check embedding dimension
		if embedding.len() != self.text_vector_dim {
			return Err(anyhow::anyhow!(
				"Search embedding has dimension {} but expected {}",
				embedding.len(),
				self.text_vector_dim
			));
		}

		// Open the table
		let table = self.db.open_table("text_blocks").execute().await?;

		// Check if the table has any data
		let row_count = table.count_rows(None).await?;
		if row_count == 0 {
			// No data, return empty vector
			return Ok(Vec::new());
		}

		// Check if index exists and create it if needed
		let has_index = table
			.list_indices()
			.await?
			.iter()
			.any(|idx| idx.columns == vec!["embedding"]);
		if !has_index && row_count > 256 {
			table
				.create_index(&["embedding"], Index::Auto)
				.execute()
				.await?
		}

		// Use provided max_results or default to a reasonable number
		let limit = max_results.unwrap_or(50);

		// Perform vector search
		let mut results = table
			.query()
			.nearest_to(embedding.as_slice())? // Pass as slice instead of reference to Vec
			.distance_type(DistanceType::Cosine) // Explicitly use cosine distance
			.limit(limit)
			.execute()
			.await?;

		let mut all_text_blocks = Vec::new();

		// Process all batches
		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() == 0 {
				continue;
			}

			// Extract _distance column which contains similarity scores
			let distance_column = batch
				.column_by_name("_distance")
				.ok_or_else(|| anyhow::anyhow!("Distance column not found"))?;

			let distance_array = distance_column
				.as_any()
				.downcast_ref::<Float32Array>()
				.ok_or_else(|| {
					anyhow::anyhow!("Could not downcast distance column to Float32Array")
				})?;

			// Convert distances to Vec<f32>
			let distances: Vec<f32> = (0..distance_array.len())
				.map(|i| distance_array.value(i))
				.collect();

			// Convert results to TextBlock structs
			let converter = BatchConverter::new(self.text_vector_dim);
			let mut text_blocks = converter.batch_to_text_blocks(&batch, Some(distances))?;

			all_text_blocks.append(&mut text_blocks);
		}

		if all_text_blocks.is_empty() {
			return Ok(Vec::new());
		}

		// Filter by similarity threshold if provided
		if let Some(threshold) = similarity_threshold {
			all_text_blocks.retain(|block| {
				if let Some(distance) = block.distance {
					distance <= threshold // Lower distance means higher similarity
				} else {
					true // Keep blocks without distance info
				}
			});
		}

		// Sort by relevance (distance) - lower distance means higher similarity, so sort ascending
		all_text_blocks.sort_by(|a, b| {
			match (a.distance, b.distance) {
				(Some(dist_a), Some(dist_b)) => dist_a
					.partial_cmp(&dist_b)
					.unwrap_or(std::cmp::Ordering::Equal),
				(Some(_), None) => std::cmp::Ordering::Less, // Items with distance come first
				(None, Some(_)) => std::cmp::Ordering::Greater, // Items without distance come last
				(None, None) => std::cmp::Ordering::Equal,   // Equal if both have no distance
			}
		});

		Ok(all_text_blocks)
	}

	pub async fn get_code_block_by_symbol(&self, symbol: &str) -> Result<Option<CodeBlock>> {
		// Open the table
		let table = self.db.open_table("code_blocks").execute().await?;

		// Check if the table has any data
		let row_count = table.count_rows(None).await?;
		if row_count == 0 {
			// No data, return None
			return Ok(None);
		}

		// Filter by symbols using LIKE for substring match
		let mut results = table
			.query()
			.only_if(format!("symbols LIKE '%{}%'", symbol))
			.limit(1)
			.execute()
			.await?;

		// Process all batches (though we expect only one with limit 1)
		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() > 0 {
				// Convert results to CodeBlock structs
				let converter = BatchConverter::new(self.code_vector_dim);
				let code_blocks = converter.batch_to_code_blocks(&batch, None)?;

				// Return the first (and only) code block
				return Ok(code_blocks.into_iter().next());
			}
		}

		Ok(None)
	}

	// Remove all blocks associated with a file path
	pub async fn remove_blocks_by_path(&self, file_path: &str) -> Result<()> {
		// Check if tables exist
		let table_names = self.db.table_names().execute().await?;
		let mut total_removed = 0;
		let mut errors = Vec::new();

		// Escape single quotes in file_path to prevent SQL injection/errors
		let escaped_path = file_path.replace("'", "''");

		// Delete from code_blocks table if it exists
		if table_names.contains(&"code_blocks".to_string()) {
			let code_blocks_table = self.db.open_table("code_blocks").execute().await?;

			// First count how many we're going to delete
			let mut count_results = code_blocks_table
				.query()
				.only_if(format!("path = '{}'", escaped_path))
				.select(lancedb::query::Select::Columns(vec!["path".to_string()]))
				.execute()
				.await?;

			let mut count = 0;
			while let Some(batch) = count_results.try_next().await? {
				count += batch.num_rows();
			}

			// Now delete them
			match code_blocks_table
				.delete(&format!("path = '{}'", escaped_path))
				.await
			{
				Ok(_) => {
					if count > 0 {
						total_removed += count;
					}
				}
				Err(e) => {
					let error_msg = format!("Error removing code blocks for {}: {}", file_path, e);
					eprintln!("{}", error_msg);
					errors.push(error_msg);
				}
			}
		}

		// Delete from text_blocks table if it exists
		if table_names.contains(&"text_blocks".to_string()) {
			let text_blocks_table = self.db.open_table("text_blocks").execute().await?;

			// For text blocks, delete exact match first
			let mut count_results = text_blocks_table
				.query()
				.only_if(format!("path = '{}'", escaped_path))
				.select(lancedb::query::Select::Columns(vec!["path".to_string()]))
				.execute()
				.await?;

			let mut exact_count = 0;
			while let Some(batch) = count_results.try_next().await? {
				exact_count += batch.num_rows();
			}

			match text_blocks_table
				.delete(&format!("path = '{}'", escaped_path))
				.await
			{
				Ok(_) => {
					if exact_count > 0 {
						total_removed += exact_count;
					}
				}
				Err(e) => {
					let error_msg =
						format!("Error removing exact text blocks for {}: {}", file_path, e);
					eprintln!("{}", error_msg);
					errors.push(error_msg);
				}
			}

			// Also delete chunked versions (path starts with file_path and contains '#')
			let mut chunked_count_results = text_blocks_table
				.query()
				.only_if(format!("path LIKE '{}#%'", escaped_path))
				.select(lancedb::query::Select::Columns(vec!["path".to_string()]))
				.execute()
				.await?;

			let mut chunked_count = 0;
			while let Some(batch) = chunked_count_results.try_next().await? {
				chunked_count += batch.num_rows();
			}

			match text_blocks_table
				.delete(&format!("path LIKE '{}#%'", escaped_path))
				.await
			{
				Ok(_) => {
					if chunked_count > 0 {
						total_removed += chunked_count;
					}
				}
				Err(e) => {
					let error_msg = format!(
						"Error removing chunked text blocks for {}: {}",
						file_path, e
					);
					eprintln!("{}", error_msg);
					errors.push(error_msg);
				}
			}
		}

		// Delete from document_blocks table if it exists
		if table_names.contains(&"document_blocks".to_string()) {
			let document_blocks_table = self.db.open_table("document_blocks").execute().await?;

			let mut count_results = document_blocks_table
				.query()
				.only_if(format!("path = '{}'", escaped_path))
				.select(lancedb::query::Select::Columns(vec!["path".to_string()]))
				.execute()
				.await?;

			let mut count = 0;
			while let Some(batch) = count_results.try_next().await? {
				count += batch.num_rows();
			}

			match document_blocks_table
				.delete(&format!("path = '{}'", escaped_path))
				.await
			{
				Ok(_) => {
					if count > 0 {
						total_removed += count;
					}
				}
				Err(e) => {
					let error_msg =
						format!("Error removing document blocks for {}: {}", file_path, e);
					eprintln!("{}", error_msg);
					errors.push(error_msg);
				}
			}
		}

		// Only report if there were significant actions or errors
		if total_removed > 0 || !errors.is_empty() {
			if total_removed > 0 {
				// Only show summary for significant removals
			}

			// Always report errors
			if !errors.is_empty() {
				eprintln!(
					"Encountered {} errors during deletion for file {}",
					errors.len(),
					file_path
				);
				for error in &errors {
					eprintln!("  - {}", error);
				}
			}
		}

		Ok(())
	}

	// Remove specific blocks by their hashes
	pub async fn remove_blocks_by_hashes(&self, hashes: &[String], table_name: &str) -> Result<()> {
		if hashes.is_empty() {
			return Ok(());
		}

		// Check if table exists
		let table_names = self.db.table_names().execute().await?;
		if !table_names.contains(&table_name.to_string()) {
			return Ok(());
		}

		let table = self.db.open_table(table_name).execute().await?;

		// Build a SQL IN clause for the hashes
		let hash_list = hashes
			.iter()
			.map(|h| format!("'{}'", h))
			.collect::<Vec<_>>()
			.join(", ");

		table.delete(&format!("hash IN ({})", hash_list)).await?;
		Ok(())
	}

	// Get all blocks for a specific file path from a table
	pub async fn get_file_blocks_metadata(
		&self,
		file_path: &str,
		table_name: &str,
	) -> Result<Vec<String>> {
		// Check if table exists
		let table_names = self.db.table_names().execute().await?;
		if !table_names.contains(&table_name.to_string()) {
			return Ok(Vec::new());
		}

		let table = self.db.open_table(table_name).execute().await?;

		// Query for blocks from this file - we only need the hash column
		let mut results = table
			.query()
			.only_if(format!("path = '{}'", file_path))
			.select(Select::Columns(vec!["hash".to_string()]))
			.execute()
			.await?;

		let mut hashes = Vec::new();

		// Process all batches
		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() == 0 {
				continue;
			}

			// Extract hashes
			let hash_array = batch
				.column_by_name("hash")
				.ok_or_else(|| anyhow::anyhow!("Hash column not found"))?
				.as_any()
				.downcast_ref::<StringArray>()
				.ok_or_else(|| anyhow::anyhow!("Hash column is not a StringArray"))?;

			for i in 0..hash_array.len() {
				hashes.push(hash_array.value(i).to_string());
			}
		}

		Ok(hashes)
	}

	// Get all file paths from the database
	pub async fn get_all_indexed_file_paths(&self) -> Result<std::collections::HashSet<String>> {
		let mut all_paths = std::collections::HashSet::new();
		let table_names = self.db.table_names().execute().await?;

		// Check code_blocks table
		if table_names.contains(&"code_blocks".to_string()) {
			let table = self.db.open_table("code_blocks").execute().await?;
			let mut results = table
				.query()
				.select(Select::Columns(vec!["path".to_string()]))
				.execute()
				.await?;

			// Process all batches
			while let Some(batch) = results.try_next().await? {
				if batch.num_rows() > 0 {
					let path_array = batch
						.column_by_name("path")
						.ok_or_else(|| anyhow::anyhow!("Path column not found"))?
						.as_any()
						.downcast_ref::<StringArray>()
						.ok_or_else(|| anyhow::anyhow!("Path column is not a StringArray"))?;

					for i in 0..path_array.len() {
						all_paths.insert(path_array.value(i).to_string());
					}
				}
			}
		}

		// Check text_blocks table
		if table_names.contains(&"text_blocks".to_string()) {
			let table = self.db.open_table("text_blocks").execute().await?;
			let mut results = table
				.query()
				.select(Select::Columns(vec!["path".to_string()]))
				.execute()
				.await?;

			// Process all batches
			while let Some(batch) = results.try_next().await? {
				if batch.num_rows() > 0 {
					let path_array = batch
						.column_by_name("path")
						.ok_or_else(|| anyhow::anyhow!("Path column not found"))?
						.as_any()
						.downcast_ref::<StringArray>()
						.ok_or_else(|| anyhow::anyhow!("Path column is not a StringArray"))?;

					for i in 0..path_array.len() {
						let path = path_array.value(i).to_string();
						// Since text blocks now store clean paths, no need to extract base path
						// But handle legacy data that might still have chunk suffixes
						let base_path = if path.contains('#') {
							// Legacy chunked path - extract base path
							if let Some(hash_pos) = path.find('#') {
								path[..hash_pos].to_string()
							} else {
								path
							}
						} else {
							// New clean path format
							path
						};
						all_paths.insert(base_path);
					}
				}
			}
		}

		// Check document_blocks table
		if table_names.contains(&"document_blocks".to_string()) {
			let table = self.db.open_table("document_blocks").execute().await?;
			let mut results = table
				.query()
				.select(Select::Columns(vec!["path".to_string()]))
				.execute()
				.await?;

			// Process all batches
			while let Some(batch) = results.try_next().await? {
				if batch.num_rows() > 0 {
					let path_array = batch
						.column_by_name("path")
						.ok_or_else(|| anyhow::anyhow!("Path column not found"))?
						.as_any()
						.downcast_ref::<StringArray>()
						.ok_or_else(|| anyhow::anyhow!("Path column is not a StringArray"))?;

					for i in 0..path_array.len() {
						all_paths.insert(path_array.value(i).to_string());
					}
				}
			}
		}

		Ok(all_paths)
	}

	pub async fn get_code_block_by_hash(&self, hash: &str) -> Result<CodeBlock> {
		// Open the table
		let table = self.db.open_table("code_blocks").execute().await?;

		// Check if the table has any data
		let row_count = table.count_rows(None).await?;
		if row_count == 0 {
			// No data, return error
			return Err(anyhow::anyhow!("No data in code_blocks table"));
		}

		// Filter by hash for exact match
		let mut results = table
			.query()
			.only_if(format!("hash = '{}'", hash))
			.limit(1)
			.execute()
			.await?;

		// Process all batches (though we expect only one with limit 1)
		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() > 0 {
				// Convert results to CodeBlock structs
				let converter = BatchConverter::new(self.code_vector_dim);
				let code_blocks = converter.batch_to_code_blocks(&batch, None)?;

				// Return the first (and only) code block
				return code_blocks
					.into_iter()
					.next()
					.ok_or_else(|| anyhow::anyhow!("Failed to convert result to CodeBlock"));
			}
		}

		Err(anyhow::anyhow!("Code block with hash {} not found", hash))
	}

	// Flush the database to ensure all data is persisted
	pub async fn flush(&self) -> Result<()> {
		// Get all tables
		let table_names = self.db.table_names().execute().await?;

		// Open and flush each table by performing operations that force persistence
		for table_name in table_names {
			let table = self.db.open_table(&table_name).execute().await?;
			
			// Perform operations to ensure any pending writes are flushed:
			// 1. Count rows to force read access and ensure consistency
			let row_count = table.count_rows(None).await?;
			
			// 2. For tables with data, also check schema to ensure metadata is flushed
			if row_count > 0 {
				let _ = table.schema().await?;
			}
			
			// Log flush activity in debug mode for troubleshooting
			if cfg!(debug_assertions) {
				println!("Flushed table '{}' with {} rows", table_name, row_count);
			}
		}

		Ok(())
	}

	// Close the database connection explicitly (for debugging or cleanup)
	pub async fn close(self) -> Result<()> {
		// The database connection is closed automatically when the Store is dropped
		// This method is provided for explicit control over connection lifetime
		Ok(())
	}

	// Clear all tables (drop tables completely to reset schema)
	pub async fn clear_all_tables(&self) -> Result<()> {
		// Get table names
		let table_names = self.db.table_names().execute().await?;

		// Drop each table completely (this removes both data and schema)
		for table_name in table_names {
			if let Err(e) = self.db.drop_table(&table_name).await {
				eprintln!("Warning: Failed to drop table {}: {}", table_name, e);
			} else {
				println!("Dropped table: {}", table_name);
			}
		}

		Ok(())
	}

	// Check if tables exist in the database
	pub async fn tables_exist(&self, table_names: &[&str]) -> Result<bool> {
		let existing_tables = self.db.table_names().execute().await?;
		for table in table_names {
			if !existing_tables.contains(&table.to_string()) {
				return Ok(false);
			}
		}
		Ok(true)
	}

	// Store graph nodes in the database
	pub async fn store_graph_nodes(&self, node_batch: RecordBatch) -> Result<()> {
		// Open or create the table
		let table = self.db.open_table("graphrag_nodes").execute().await?;

		// Create an iterator that yields this single batch
		use std::iter::once;
		let schema = node_batch.schema();
		let batches = once(Ok(node_batch));
		let batch_reader = arrow::record_batch::RecordBatchIterator::new(batches, schema);

		// Add the batch to the table
		table.add(batch_reader).execute().await?;

		// Check if index exists and create it if needed
		let has_index = table
			.list_indices()
			.await?
			.iter()
			.any(|idx| idx.columns == vec!["embedding"]);
		let row_count = table.count_rows(None).await?;
		if !has_index && row_count > 256 {
			table
				.create_index(&["embedding"], Index::Auto)
				.execute()
				.await?
		}

		Ok(())
	}

	// Store graph relationships in the database
	pub async fn store_graph_relationships(&self, rel_batch: RecordBatch) -> Result<()> {
		// Open or create the table
		let table = self
			.db
			.open_table("graphrag_relationships")
			.execute()
			.await?;

		// Create an iterator that yields this single batch
		use std::iter::once;
		let schema = rel_batch.schema();
		let batches = once(Ok(rel_batch));
		let batch_reader = arrow::record_batch::RecordBatchIterator::new(batches, schema);

		// Add the batch to the table
		table.add(batch_reader).execute().await?;

		Ok(())
	}

	// Clear all graph nodes from the database
	pub async fn clear_graph_nodes(&self) -> Result<()> {
		let table_names = self.db.table_names().execute().await?;
		if table_names.contains(&"graphrag_nodes".to_string()) {
			let table = self.db.open_table("graphrag_nodes").execute().await?;
			table.delete("TRUE").await?;
		}
		Ok(())
	}

	// Clear all graph relationships from the database
	pub async fn clear_graph_relationships(&self) -> Result<()> {
		let table_names = self.db.table_names().execute().await?;
		if table_names.contains(&"graphrag_relationships".to_string()) {
			let table = self
				.db
				.open_table("graphrag_relationships")
				.execute()
				.await?;
			table.delete("TRUE").await?;
		}
		Ok(())
	}

	// Search for graph nodes by vector similarity
	pub async fn search_graph_nodes(&self, embedding: &[f32], limit: usize) -> Result<RecordBatch> {
		// Check embedding dimension
		if embedding.len() != self.code_vector_dim {
			return Err(anyhow::anyhow!(
				"Search embedding has dimension {} but expected {}",
				embedding.len(),
				self.code_vector_dim
			));
		}

		// Open the table
		let table = self.db.open_table("graphrag_nodes").execute().await?;

		// Check if the table has any data
		let row_count = table.count_rows(None).await?;
		if row_count == 0 {
			// Create an empty record batch with the right schema
			let schema = Arc::new(Schema::new(vec![
				Field::new("id", DataType::Utf8, false),
				Field::new("name", DataType::Utf8, false),
				Field::new("kind", DataType::Utf8, false),
				Field::new("path", DataType::Utf8, false),
				Field::new("description", DataType::Utf8, false),
				Field::new("symbols", DataType::Utf8, true),
				Field::new("hash", DataType::Utf8, false),
				Field::new(
					"embedding",
					DataType::FixedSizeList(
						Arc::new(Field::new("item", DataType::Float32, true)),
						self.code_vector_dim as i32,
					),
					true,
				),
			]));
			return Ok(RecordBatch::new_empty(schema));
		}

		// Check if index exists and create it if needed
		let has_index = table
			.list_indices()
			.await?
			.iter()
			.any(|idx| idx.columns == vec!["embedding"]);
		if !has_index && row_count > 256 {
			table
				.create_index(&["embedding"], Index::Auto)
				.execute()
				.await?;
		}

		// Perform vector search
		let mut results = table
			.query()
			.nearest_to(embedding)? // Vector search
			.distance_type(DistanceType::Cosine) // Explicitly use cosine distance
			.limit(limit) // No conversion needed
			.execute()
			.await?;

		// Process all batches - for this function we need to return a single RecordBatch
		// so we'll collect all results and combine them or return the first non-empty one
		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() > 0 {
				return Ok(batch);
			}
		}

		// If no results, create an empty record batch with the right schema
		let schema = Arc::new(Schema::new(vec![
			Field::new("id", DataType::Utf8, false),
			Field::new("name", DataType::Utf8, false),
			Field::new("kind", DataType::Utf8, false),
			Field::new("path", DataType::Utf8, false),
			Field::new("description", DataType::Utf8, false),
			Field::new("symbols", DataType::Utf8, true),
			Field::new("hash", DataType::Utf8, false),
			Field::new(
				"embedding",
				DataType::FixedSizeList(
					Arc::new(Field::new("item", DataType::Float32, true)),
					self.code_vector_dim as i32,
				),
				true,
			),
		]));
		Ok(RecordBatch::new_empty(schema))
	}

	// Get all graph relationships
	pub async fn get_graph_relationships(&self) -> Result<RecordBatch> {
		// Open the table
		let table_names = self.db.table_names().execute().await?;
		if !table_names.contains(&"graphrag_relationships".to_string()) {
			// Return empty batch with schema
			let schema = Arc::new(Schema::new(vec![
				Field::new("id", DataType::Utf8, false),
				Field::new("source", DataType::Utf8, false),
				Field::new("target", DataType::Utf8, false),
				Field::new("relation_type", DataType::Utf8, false),
				Field::new("description", DataType::Utf8, false),
				Field::new("confidence", DataType::Float32, false),
			]));
			return Ok(RecordBatch::new_empty(schema));
		}

		let table = self
			.db
			.open_table("graphrag_relationships")
			.execute()
			.await?;

		// Get all relationships
		let mut results = table.query().execute().await?;

		// Process all batches - for this function we need to return a single RecordBatch
		// so we'll return the first non-empty batch
		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() > 0 {
				return Ok(batch);
			}
		}

		// If no results, return empty batch with schema
		let schema = Arc::new(Schema::new(vec![
			Field::new("id", DataType::Utf8, false),
			Field::new("source", DataType::Utf8, false),
			Field::new("target", DataType::Utf8, false),
			Field::new("relation_type", DataType::Utf8, false),
			Field::new("description", DataType::Utf8, false),
			Field::new("confidence", DataType::Float32, false),
		]));
		Ok(RecordBatch::new_empty(schema))
	}

	pub fn get_code_vector_dim(&self) -> usize {
		self.code_vector_dim
	}

	// Store git metadata (commit hash, etc.)
	pub async fn store_git_metadata(&self, commit_hash: &str) -> Result<()> {
		// Check if table exists, create if not
		let table_names = self.db.table_names().execute().await?;
		if !table_names.contains(&"git_metadata".to_string()) {
			self.create_git_metadata_table().await?;
		}

		let table = self.db.open_table("git_metadata").execute().await?;

		// Delete existing records (we only store one)
		table.delete("TRUE").await?;

		// Create new record batch
		let schema = Arc::new(Schema::new(vec![
			Field::new("commit_hash", DataType::Utf8, false),
			Field::new("indexed_at", DataType::Int64, false),
		]));

		let batch = RecordBatch::try_new(
			schema.clone(),
			vec![
				Arc::new(StringArray::from(vec![commit_hash])),
				Arc::new(Int64Array::from(vec![std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.unwrap()
					.as_secs() as i64])),
			],
		)?;

		// Create an iterator that yields this single batch
		use std::iter::once;
		let batches = once(Ok(batch));
		let batch_reader = arrow::record_batch::RecordBatchIterator::new(batches, schema);

		table.add(batch_reader).execute().await?;
		Ok(())
	}

	// Get last indexed git commit hash
	pub async fn get_last_commit_hash(&self) -> Result<Option<String>> {
		let table_names = self.db.table_names().execute().await?;
		if !table_names.contains(&"git_metadata".to_string()) {
			return Ok(None);
		}

		let table = self.db.open_table("git_metadata").execute().await?;

		let mut results = table
			.query()
			.select(Select::Columns(vec!["commit_hash".to_string()]))
			.limit(1)
			.execute()
			.await?;

		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() > 0 {
				if let Some(commit_array) = batch.column_by_name("commit_hash") {
					if let Some(string_array) = commit_array.as_any().downcast_ref::<StringArray>()
					{
						return Ok(Some(string_array.value(0).to_string()));
					}
				}
			}
		}

		Ok(None)
	}

	// Create git metadata table
	async fn create_git_metadata_table(&self) -> Result<()> {
		let schema = Arc::new(Schema::new(vec![
			Field::new("commit_hash", DataType::Utf8, false),
			Field::new("indexed_at", DataType::Int64, false),
		]));

		let _table = self
			.db
			.create_empty_table("git_metadata", schema)
			.execute()
			.await?;
		Ok(())
	}

	// Store file metadata (modification time, etc.)
	pub async fn store_file_metadata(&self, file_path: &str, mtime: u64) -> Result<()> {
		// Check if table exists, create if not
		let table_names = self.db.table_names().execute().await?;
		if !table_names.contains(&"file_metadata".to_string()) {
			self.create_file_metadata_table().await?;
		}

		let table = self.db.open_table("file_metadata").execute().await?;

		// Delete existing record for this file path
		table
			.delete(&format!("path = '{}'", file_path.replace("'", "''")))
			.await?;

		// Create new record batch
		let schema = Arc::new(Schema::new(vec![
			Field::new("path", DataType::Utf8, false),
			Field::new("mtime", DataType::Int64, false),
			Field::new("indexed_at", DataType::Int64, false),
		]));

		let batch = RecordBatch::try_new(
			schema.clone(),
			vec![
				Arc::new(StringArray::from(vec![file_path])),
				Arc::new(Int64Array::from(vec![mtime as i64])),
				Arc::new(Int64Array::from(vec![std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.unwrap()
					.as_secs() as i64])),
			],
		)?;

		// Create an iterator that yields this single batch
		use std::iter::once;
		let batches = once(Ok(batch));
		let batch_reader = arrow::record_batch::RecordBatchIterator::new(batches, schema);

		table.add(batch_reader).execute().await?;
		Ok(())
	}

	// Get file modification time from metadata
	pub async fn get_file_mtime(&self, file_path: &str) -> Result<Option<u64>> {
		let table_names = self.db.table_names().execute().await?;
		if !table_names.contains(&"file_metadata".to_string()) {
			return Ok(None);
		}

		let table = self.db.open_table("file_metadata").execute().await?;

		let mut results = table
			.query()
			.only_if(format!("path = '{}'", file_path.replace("'", "''")))
			.select(Select::Columns(vec!["mtime".to_string()]))
			.limit(1)
			.execute()
			.await?;

		while let Some(batch) = results.try_next().await? {
			if batch.num_rows() > 0 {
				if let Some(mtime_array) = batch.column_by_name("mtime") {
					if let Some(int_array) = mtime_array.as_any().downcast_ref::<Int64Array>() {
						let mtime = int_array.value(0);
						return Ok(Some(mtime as u64));
					}
				}
			}
		}

		Ok(None)
	}

	// Create file metadata table
	async fn create_file_metadata_table(&self) -> Result<()> {
		let schema = Arc::new(Schema::new(vec![
			Field::new("path", DataType::Utf8, false),
			Field::new("mtime", DataType::Int64, false),
			Field::new("indexed_at", DataType::Int64, false),
		]));

		let _table = self
			.db
			.create_empty_table("file_metadata", schema)
			.execute()
			.await?;
		Ok(())
	}

	// Debug function to list all files currently in the database
	pub async fn debug_list_all_files(&self) -> Result<()> {
		let table_names = self.db.table_names().execute().await?;

		for table_name in &["code_blocks", "text_blocks", "document_blocks"] {
			if table_names.contains(&table_name.to_string()) {
				println!("\n=== Files in {} table ===", table_name);
				let table = self.db.open_table(*table_name).execute().await?;
				let mut results = table
					.query()
					.select(lancedb::query::Select::Columns(vec!["path".to_string()]))
					.execute()
					.await?;

				let mut unique_paths = std::collections::HashSet::new();

				// Process all result batches
				while let Some(batch) = results.try_next().await? {
					if batch.num_rows() > 0 {
						if let Some(column) = batch.column_by_name("path") {
							if let Some(path_array) = column.as_any().downcast_ref::<StringArray>()
							{
								for i in 0..path_array.len() {
									let path = path_array.value(i).to_string();
									unique_paths.insert(path);
								}
							} else {
								return Err(anyhow::anyhow!("Path column is not a StringArray"));
							}
						} else {
							return Err(anyhow::anyhow!("Path column not found"));
						}
					}
				}

				if !unique_paths.is_empty() {
					let count = unique_paths.len();
					for path in unique_paths {
						println!("  - {}", path);
					}
					println!("Total unique files: {}", count);
				} else {
					println!("  (no files)");
				}
			} else {
				println!("\n=== Table {} does not exist ===", table_name);
			}
		}
		println!();
		Ok(())
	}
}
