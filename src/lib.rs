// Main lib.rs file that exports our modules
pub mod config;
pub mod embedding;
pub mod indexer;
pub mod memory;
pub mod mcp;
pub mod store;
pub mod state;
pub mod reranker;

// Re-export commonly used items for convenience
pub use config::Config;
pub use store::Store;
