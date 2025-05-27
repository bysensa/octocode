//! MCP (Model Context Protocol) server implementation
//!
//! This module provides a modular MCP server with separate tool providers:
//! - SemanticCodeProvider: Semantic code search functionality
//! - GraphRagProvider: GraphRAG relationship-aware search
//! - MemoryProvider: AI memory storage and retrieval
//!
//! The server automatically enables available tools based on configuration.

pub mod types;
pub mod semantic_code;
pub mod graphrag;
pub mod memory;
pub mod server;

pub use server::McpServer;
