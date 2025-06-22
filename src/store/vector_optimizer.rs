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

//! Intelligent vector index optimization for LanceDB
//!
//! This module automatically tunes vector index parameters based on:
//! - Dataset size and characteristics
//! - Vector dimensions
//! - System capabilities
//! - LanceDB best practices

use lancedb::DistanceType;

/// Vector index optimization parameters automatically calculated from dataset characteristics
#[derive(Debug, Clone)]
pub struct VectorIndexParams {
	pub should_create_index: bool,
	pub num_partitions: u32,
	pub num_sub_vectors: u32,
	pub num_bits: u8,
	pub distance_type: DistanceType,
}

/// Search optimization parameters for vector queries
#[derive(Debug, Clone)]
pub struct SearchParams {
	pub nprobes: usize,
	pub refine_factor: Option<u32>,
}

/// Intelligent vector index optimizer
pub struct VectorOptimizer;

impl VectorOptimizer {
	/// Calculate optimal index parameters based on dataset characteristics
	///
	/// Based on LanceDB documentation and best practices:
	/// - For datasets < 1000 rows: No index (brute force is faster)
	/// - For datasets 1000-100K rows: Create index with conservative settings
	/// - For datasets > 100K rows: Create index with aggressive optimization
	///
	/// # Arguments
	/// * `row_count` - Number of rows in the dataset
	/// * `vector_dimension` - Dimension of the vectors
	///
	/// # Returns
	/// Optimized index parameters or recommendation to skip indexing
	pub fn calculate_index_params(row_count: usize, vector_dimension: usize) -> VectorIndexParams {
		// LanceDB performs excellently with brute force search up to ~100K rows
		// For smaller datasets, indexing overhead outweighs benefits
		if row_count < 1000 {
			tracing::debug!(
				"Dataset size {} is small, skipping index creation (brute force will be faster)",
				row_count
			);
			return VectorIndexParams {
				should_create_index: false,
				num_partitions: 0,
				num_sub_vectors: 0,
				num_bits: 8,
				distance_type: DistanceType::Cosine,
			};
		}

		// Calculate optimal number of partitions
		// Rule: sqrt(num_rows) with 4K-8K rows per partition for optimal I/O
		let sqrt_rows = (row_count as f64).sqrt() as u32;
		let optimal_partition_size = if row_count < 10_000 {
			// For smaller datasets, use fewer partitions to avoid over-partitioning
			std::cmp::max(sqrt_rows / 2, 2)
		} else if row_count < 100_000 {
			// Standard sqrt rule
			sqrt_rows
		} else {
			// For large datasets, ensure partitions don't get too big
			let max_partition_size = 8000;
			std::cmp::max(row_count as u32 / max_partition_size, sqrt_rows)
		};

		// Clamp partitions to reasonable bounds
		let num_partitions = optimal_partition_size.clamp(2, 1024);

		// Calculate optimal number of sub-vectors for Product Quantization
		// Rule: dimension / 16, but ensure it's a factor of dimension and multiple of 8 for SIMD
		let base_sub_vectors = std::cmp::max(1, vector_dimension / 16);
		let num_sub_vectors = Self::find_optimal_sub_vectors(vector_dimension, base_sub_vectors);

		// Choose number of bits based on dataset size and performance requirements
		let num_bits = if row_count > 50_000 {
			// For large datasets, use 8 bits for better accuracy
			8
		} else {
			// For smaller datasets, 8 bits is still recommended for good quality
			8
		};

		tracing::debug!(
			"Calculated index params for {} rows, {} dimensions: partitions={}, sub_vectors={}, bits={}",
			row_count, vector_dimension, num_partitions, num_sub_vectors, num_bits
		);

		VectorIndexParams {
			should_create_index: true,
			num_partitions,
			num_sub_vectors,
			num_bits,
			distance_type: DistanceType::Cosine, // Cosine is generally best for semantic similarity
		}
	}

	/// Calculate optimal search parameters based on index characteristics
	///
	/// # Arguments
	/// * `num_partitions` - Number of partitions in the index
	/// * `row_count` - Total number of rows
	///
	/// # Returns
	/// Optimized search parameters for best recall/latency balance
	pub fn calculate_search_params(num_partitions: u32, row_count: usize) -> SearchParams {
		// Calculate optimal nprobes (5-15% of partitions)
		let min_nprobes = std::cmp::max(1, num_partitions / 20); // 5%
		let max_nprobes = std::cmp::max(min_nprobes, num_partitions / 7); // ~15%

		// For smaller datasets, search more partitions for better recall
		let nprobes = if row_count < 10_000 {
			max_nprobes as usize
		} else {
			// Standard rule: ~10% of partitions
			std::cmp::max(min_nprobes, num_partitions / 10) as usize
		};

		// Calculate refine factor based on dataset size
		let refine_factor = if row_count > 100_000 {
			// For large datasets, use refine factor for better accuracy
			Some(20)
		} else if row_count > 10_000 {
			// For medium datasets, moderate refine factor
			Some(10)
		} else {
			// For small datasets, skip refine factor to avoid overhead
			None
		};

		tracing::debug!(
			"Calculated search params for {} partitions, {} rows: nprobes={}, refine_factor={:?}",
			num_partitions,
			row_count,
			nprobes,
			refine_factor
		);

		SearchParams {
			nprobes,
			refine_factor,
		}
	}

	/// Find optimal number of sub-vectors that is a factor of dimension and SIMD-friendly
	///
	/// # Arguments
	/// * `dimension` - Vector dimension
	/// * `target` - Target number of sub-vectors
	///
	/// # Returns
	/// Optimal number of sub-vectors
	fn find_optimal_sub_vectors(dimension: usize, target: usize) -> u32 {
		// Find the largest factor of dimension that is <= target and gives reasonable sub-vector size
		let mut best = 1;

		// Iterate in reverse order to find the largest valid factor first
		for candidate in (1..=target).rev() {
			if dimension % candidate == 0 {
				// Check if resulting sub-vector size is reasonable for PQ
				let sub_vector_size = dimension / candidate;
				// Accept sub-vector sizes that are multiples of 4 or <= 8 for good PQ performance
				if sub_vector_size % 4 == 0 || sub_vector_size <= 8 {
					best = candidate;
					break; // Take the first (largest) valid candidate
				}
			}
		}

		// If no good factor found, find the closest factor
		if best == 1 {
			for candidate in (target..=dimension).rev() {
				if dimension % candidate == 0 {
					best = candidate;
					break;
				}
			}
		}

		// Ensure we have at least 1 and at most dimension
		std::cmp::max(1, std::cmp::min(best, dimension)) as u32
	}

	/// Determine if index should be recreated based on current parameters vs optimal
	///
	/// # Arguments
	/// * `current_partitions` - Current number of partitions
	/// * `current_sub_vectors` - Current number of sub-vectors
	/// * `optimal` - Optimal parameters for current dataset
	///
	/// # Returns
	/// True if index should be recreated for better performance
	pub fn should_recreate_index(
		current_partitions: u32,
		current_sub_vectors: u32,
		optimal: &VectorIndexParams,
	) -> bool {
		if !optimal.should_create_index {
			return false;
		}

		// Recreate if parameters are significantly different
		let partition_diff = (current_partitions as f32 - optimal.num_partitions as f32).abs()
			/ optimal.num_partitions as f32;
		let sub_vector_diff = (current_sub_vectors as f32 - optimal.num_sub_vectors as f32).abs()
			/ optimal.num_sub_vectors as f32;

		// Recreate if difference is > 50% for partitions or > 25% for sub-vectors
		partition_diff > 0.5 || sub_vector_diff > 0.25
	}

	/// Check if index needs optimization based on dataset growth
	///
	/// # Arguments
	/// * `current_rows` - Current number of rows in the table
	/// * `vector_dimension` - Vector dimension
	/// * `has_embedding_index` - Whether an embedding index already exists
	///
	/// # Returns
	/// True if index should be recreated due to significant dataset growth
	pub fn should_optimize_for_growth(
		current_rows: usize,
		vector_dimension: usize,
		has_embedding_index: bool,
	) -> bool {
		if !has_embedding_index {
			return false;
		}

		// Calculate optimal parameters for current dataset size
		let optimal_params = Self::calculate_index_params(current_rows, vector_dimension);

		if !optimal_params.should_create_index {
			return false;
		}

		// Simple heuristic: optimize every time dataset reaches certain growth milestones
		// This ensures index stays reasonably optimal as data grows
		let growth_milestones = [
			1000, 5000, 10000, 25000, 50000, 100000, 250000, 500000, 1000000,
		];

		// Check if we are exactly at or very close to a significant growth milestone
		for &milestone in &growth_milestones {
			// Only optimize within a small window around the milestone (±50 rows)
			if current_rows >= milestone && current_rows <= milestone + 50 {
				tracing::info!(
					"Dataset reached {} rows milestone, considering index optimization",
					milestone
				);
				return true;
			}
		}

		// Additional check: if dataset is very large, optimize every 100k rows (within small window)
		if current_rows > 1000000 && current_rows % 100000 <= 50 {
			return true;
		}

		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_small_dataset_no_index() {
		let params = VectorOptimizer::calculate_index_params(500, 768);
		assert!(!params.should_create_index);
	}

	#[test]
	fn test_medium_dataset_creates_index() {
		let params = VectorOptimizer::calculate_index_params(5000, 768);
		assert!(params.should_create_index);
		assert!(params.num_partitions >= 2);
		assert!(params.num_sub_vectors >= 1);
		assert_eq!(params.num_bits, 8);
	}

	#[test]
	fn test_large_dataset_optimized() {
		let params = VectorOptimizer::calculate_index_params(200_000, 1536);
		assert!(params.should_create_index);
		assert!(params.num_partitions > 100); // Should have many partitions for large dataset
		assert!(params.num_sub_vectors > 50); // Should have many sub-vectors for high dimension
	}

	#[test]
	fn test_sub_vector_calculation() {
		// Test with dimension that's divisible by 16
		assert_eq!(VectorOptimizer::find_optimal_sub_vectors(768, 48), 48);

		// Test with dimension that's not perfectly divisible
		assert_eq!(VectorOptimizer::find_optimal_sub_vectors(1000, 62), 50); // 1000/50 = 20, which is good
	}

	#[test]
	fn test_search_params() {
		let search_params = VectorOptimizer::calculate_search_params(100, 50_000);
		assert!(search_params.nprobes >= 5); // At least 5% of partitions
		assert!(search_params.nprobes <= 15); // At most 15% of partitions
		assert!(search_params.refine_factor.is_some());
	}

	#[test]
	fn test_growth_optimization() {
		// Should optimize at growth milestones when index exists
		assert!(VectorOptimizer::should_optimize_for_growth(1000, 768, true));
		assert!(VectorOptimizer::should_optimize_for_growth(5000, 768, true));
		assert!(VectorOptimizer::should_optimize_for_growth(
			10000, 768, true
		));

		// Should not optimize between milestones
		assert!(!VectorOptimizer::should_optimize_for_growth(
			1500, 768, true
		));
		assert!(!VectorOptimizer::should_optimize_for_growth(
			7500, 768, true
		));

		// Should not optimize if no index exists
		assert!(!VectorOptimizer::should_optimize_for_growth(
			5000, 768, false
		));
	}
}
