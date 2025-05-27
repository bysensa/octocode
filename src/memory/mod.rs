// Memory module for AI context and conversation state management
// Uses LanceDB for vector storage and semantic search capabilities

pub mod types;
pub mod store;
pub mod manager;
pub mod git_utils;

// Re-export the main types and interfaces
pub use manager::{MemoryManager, MemoryStats};
pub use types::{
    Memory, MemoryType, MemoryMetadata, MemoryQuery, MemorySearchResult,
    MemoryRelationship, RelationshipType, MemoryConfig, MemorySortBy, SortOrder
};
pub use store::MemoryStore;
pub use git_utils::{GitUtils, CommitInfo};