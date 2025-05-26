// GraphRAG module entry point

pub mod ai;
pub mod builder;
pub mod database;
pub mod relationships;
pub mod types;
pub mod utils;

// Re-export the main types and interfaces for backward compatibility
pub use builder::GraphBuilder;
pub use types::{CodeGraph, CodeNode, CodeRelationship, FunctionInfo};
pub use utils::{
	cosine_similarity, detect_project_root, graphrag_nodes_to_markdown,
	render_graphrag_nodes_json, to_relative_path,
};

// GraphRAG implementation for searching (backward compatibility)
use crate::config::Config;
use anyhow::Result;

pub struct GraphRAG {
	config: Config,
}

impl GraphRAG {
	pub fn new(config: Config) -> Self {
		Self { config }
	}

	pub async fn search(&self, query: &str) -> Result<String> {
		let builder = GraphBuilder::new(self.config.clone()).await?;
		let nodes = builder.search_nodes(query).await?;
		Ok(graphrag_nodes_to_markdown(&nodes))
	}
}