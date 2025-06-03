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
use chrono::Utc;
use std::collections::HashMap;

use super::types::{Memory, MemoryConfig, MemoryQuery, MemoryRelationship, MemorySearchResult};

/// Simple in-memory storage for memories with vector search capabilities
/// This is a simplified implementation to get the memory system working
pub struct MemoryStore {
	memories: HashMap<String, Memory>,
	relationships: HashMap<String, MemoryRelationship>,
	embedding_provider: Box<dyn crate::embedding::provider::EmbeddingProvider>,
	config: MemoryConfig,
}

impl MemoryStore {
	/// Create a new memory store
	pub async fn new(
		_db_path: &str, // For future LanceDB integration
		embedding_provider: Box<dyn crate::embedding::provider::EmbeddingProvider>,
		config: MemoryConfig,
	) -> Result<Self> {
		Ok(Self {
			memories: HashMap::new(),
			relationships: HashMap::new(),
			embedding_provider,
			config,
		})
	}

	/// Store a memory
	pub async fn store_memory(&mut self, memory: &Memory) -> Result<()> {
		self.memories.insert(memory.id.clone(), memory.clone());
		Ok(())
	}

	/// Store multiple memories in batch
	pub async fn store_memories(&mut self, memories: &[Memory]) -> Result<()> {
		for memory in memories {
			self.memories.insert(memory.id.clone(), memory.clone());
		}
		Ok(())
	}

	/// Update an existing memory
	pub async fn update_memory(&mut self, memory: &Memory) -> Result<()> {
		self.memories.insert(memory.id.clone(), memory.clone());
		Ok(())
	}

	/// Delete a memory by ID
	pub async fn delete_memory(&mut self, memory_id: &str) -> Result<()> {
		self.memories.remove(memory_id);

		// Also delete any relationships involving this memory
		self.relationships
			.retain(|_, rel| rel.source_id != memory_id && rel.target_id != memory_id);

		Ok(())
	}

	/// Search memories using vector similarity and optional filters
	pub async fn search_memories(&self, query: &MemoryQuery) -> Result<Vec<MemorySearchResult>> {
		let limit = query
			.limit
			.unwrap_or(self.config.max_search_results)
			.min(self.config.max_search_results);
		let min_relevance = query.min_relevance.unwrap_or(0.0);

		let mut results = Vec::new();

		// If we have a text query, use semantic search
		if let Some(ref query_text) = query.query_text {
			let query_embedding = self
				.embedding_provider
				.generate_embedding(query_text)
				.await?;

			for memory in self.memories.values() {
				// Apply filters first
				if !self.matches_filters(memory, query) {
					continue;
				}

				// Calculate semantic similarity
				let memory_embedding = self
					.embedding_provider
					.generate_embedding(&memory.get_searchable_text())
					.await?;
				let similarity =
					self.calculate_cosine_similarity(&query_embedding, &memory_embedding);

				if similarity >= min_relevance {
					results.push(MemorySearchResult {
						memory: memory.clone(),
						relevance_score: similarity,
						selection_reason: self.generate_selection_reason(query, similarity),
					});
				}
			}
		} else {
			// No text query, just apply filters and use importance as relevance
			for memory in self.memories.values() {
				if self.matches_filters(memory, query) {
					let relevance_score = memory.metadata.importance;
					if relevance_score >= min_relevance {
						results.push(MemorySearchResult {
							memory: memory.clone(),
							relevance_score,
							selection_reason: self
								.generate_selection_reason(query, relevance_score),
						});
					}
				}
			}
		}

		// Sort by relevance score (highest first)
		results.sort_by(|a, b| {
			b.relevance_score
				.partial_cmp(&a.relevance_score)
				.unwrap_or(std::cmp::Ordering::Equal)
		});

		// Apply final limit
		results.truncate(limit);

		Ok(results)
	}

	/// Get a memory by ID
	pub async fn get_memory(&self, memory_id: &str) -> Result<Option<Memory>> {
		Ok(self.memories.get(memory_id).cloned())
	}

	/// Get all memories (paginated)
	pub async fn get_all_memories(&self, offset: usize, limit: usize) -> Result<Vec<Memory>> {
		let mut all_memories: Vec<Memory> = self.memories.values().cloned().collect();

		// Sort by creation date (most recent first)
		all_memories.sort_by(|a, b| b.created_at.cmp(&a.created_at));

		// Apply pagination
		let start = offset.min(all_memories.len());
		let end = (offset + limit).min(all_memories.len());

		Ok(all_memories[start..end].to_vec())
	}

	/// Store a memory relationship
	pub async fn store_relationship(&mut self, relationship: &MemoryRelationship) -> Result<()> {
		self.relationships
			.insert(relationship.id.clone(), relationship.clone());
		Ok(())
	}

	/// Get relationships for a memory
	pub async fn get_memory_relationships(
		&self,
		memory_id: &str,
	) -> Result<Vec<MemoryRelationship>> {
		let relationships: Vec<MemoryRelationship> = self
			.relationships
			.values()
			.filter(|rel| rel.source_id == memory_id || rel.target_id == memory_id)
			.cloned()
			.collect();

		Ok(relationships)
	}

	/// Get total count of memories
	pub async fn get_memory_count(&self) -> Result<usize> {
		Ok(self.memories.len())
	}

	/// Clean up old memories based on configuration
	pub async fn cleanup_old_memories(&mut self) -> Result<usize> {
		if let Some(cleanup_days) = self.config.auto_cleanup_days {
			let cutoff_date = Utc::now() - chrono::Duration::days(cleanup_days as i64);

			let initial_count = self.memories.len();

			self.memories.retain(|_, memory| {
				memory.created_at >= cutoff_date
					|| memory.metadata.importance >= self.config.cleanup_min_importance
			});

			let deleted_count = initial_count - self.memories.len();
			Ok(deleted_count)
		} else {
			Ok(0)
		}
	}

	/// Check if memory matches the query filters
	fn matches_filters(&self, memory: &Memory, query: &MemoryQuery) -> bool {
		// Filter by memory types
		if let Some(ref memory_types) = query.memory_types {
			if !memory_types.contains(&memory.memory_type) {
				return false;
			}
		}

		// Filter by tags (any of these tags)
		if let Some(ref tags) = query.tags {
			if !tags.iter().any(|tag| memory.metadata.tags.contains(tag)) {
				return false;
			}
		}

		// Filter by related files
		if let Some(ref files) = query.related_files {
			if !files
				.iter()
				.any(|file| memory.metadata.related_files.contains(file))
			{
				return false;
			}
		}

		// Filter by git commit
		if let Some(ref git_commit) = query.git_commit {
			if memory.metadata.git_commit.as_ref() != Some(git_commit) {
				return false;
			}
		}

		// Filter by minimum importance
		if let Some(min_importance) = query.min_importance {
			if memory.metadata.importance < min_importance {
				return false;
			}
		}

		// Filter by minimum confidence
		if let Some(min_confidence) = query.min_confidence {
			if memory.metadata.confidence < min_confidence {
				return false;
			}
		}

		// Filter by creation date range
		if let Some(created_after) = query.created_after {
			if memory.created_at < created_after {
				return false;
			}
		}

		if let Some(created_before) = query.created_before {
			if memory.created_at > created_before {
				return false;
			}
		}

		true
	}

	/// Calculate cosine similarity between two vectors
	fn calculate_cosine_similarity(&self, vec1: &[f32], vec2: &[f32]) -> f32 {
		if vec1.len() != vec2.len() {
			return 0.0;
		}

		let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
		let magnitude1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
		let magnitude2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();

		if magnitude1 == 0.0 || magnitude2 == 0.0 {
			return 0.0;
		}

		dot_product / (magnitude1 * magnitude2)
	}

	/// Generate selection reason for search results
	fn generate_selection_reason(&self, query: &MemoryQuery, relevance_score: f32) -> String {
		let mut reasons = Vec::new();

		if query.query_text.is_some() {
			reasons.push(format!("Semantic similarity: {:.2}", relevance_score));
		}

		if query.memory_types.is_some() {
			reasons.push("Matches memory type filter".to_string());
		}

		if query.tags.is_some() {
			reasons.push("Contains matching tags".to_string());
		}

		if query.related_files.is_some() {
			reasons.push("Related to specified files".to_string());
		}

		if query.git_commit.is_some() {
			reasons.push("Matches Git commit filter".to_string());
		}

		if reasons.is_empty() {
			"Matches search criteria".to_string()
		} else {
			reasons.join(", ")
		}
	}
}
