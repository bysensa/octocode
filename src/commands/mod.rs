pub mod index;
pub mod search;
pub mod view;
pub mod watch;
pub mod config;
pub mod graphrag;
pub mod clear;
pub mod mcp;
pub mod debug;

// Re-export all the command structs and enums
pub use index::IndexArgs;
pub use search::SearchArgs;
pub use view::ViewArgs;
pub use watch::WatchArgs;
pub use config::ConfigArgs;
pub use graphrag::GraphRAGArgs;
pub use mcp::McpArgs;
pub use debug::DebugArgs;
