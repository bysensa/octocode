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

// Octocode - Intelligent Code Indexer and Graph Builder
// Copyright (c) 2025 Muvon Un Limited

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

use octocode::config::Config;
use octocode::store::Store;

mod commands;

#[derive(Parser)]
#[command(name = "octocode")]
#[command(version = "0.1.0")]
#[command(about = "Octocode is a smart code indexer and search tool")]
struct OctocodeArgs {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Index the current directory's codebase
	Index(commands::IndexArgs),

	/// Search the codebase with a natural language query
	Search(commands::SearchArgs),

	/// View file signatures (functions, methods, etc.)
	View(commands::ViewArgs),

	/// Watch for changes in the codebase and reindex automatically
	Watch(commands::WatchArgs),

	/// Generate a default configuration file
	Config(commands::ConfigArgs),

	/// Query and explore the code relationship graph (GraphRAG)
	#[command(name = "graphrag")]
	GraphRAG(commands::GraphRAGArgs),

	/// Start MCP (Model Context Protocol) server
	Mcp(commands::McpArgs),

	/// Memory management for storing and retrieving information
	Memory(commands::MemoryArgs),

	/// Clear all database tables (useful for debugging)
	Clear,

	/// Debug commands for troubleshooting
	Debug(commands::DebugArgs),

	/// Generate and create git commit with AI assistance
	Commit(commands::CommitArgs),

	/// Review staged changes for best practices and potential issues
	Review(commands::ReviewArgs),

	/// Format code according to .editorconfig rules
	Format(commands::FormatArgs),

	/// View MCP server logs
	Logs(commands::LogsArgs),

	/// Generate shell completion scripts
	Completion {
		/// The shell to generate completion for
		#[arg(value_enum)]
		shell: Shell,
	},
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let args = OctocodeArgs::parse();

	// Load configuration - ensure .octocode directory exists
	let config = Config::load()?;

	// Handle the config command separately
	if let Commands::Config(config_args) = &args.command {
		return commands::config::execute(config_args, config);
	}

	// Handle the MCP command separately (doesn't need store)
	if let Commands::Mcp(mcp_args) = &args.command {
		return commands::mcp::run(mcp_args.clone()).await;
	}

	// Handle the Commit command separately (doesn't need store)
	if let Commands::Commit(commit_args) = &args.command {
		return commands::commit::execute(&config, commit_args).await;
	}

	// Handle the Review command separately (doesn't need store)
	if let Commands::Review(review_args) = &args.command {
		return commands::review::execute(&config, review_args).await;
	}

	// Handle the Format command separately (doesn't need store)
	if let Commands::Format(format_args) = &args.command {
		return commands::format::execute(format_args).await;
	}

	// Handle the Memory command separately (doesn't need store)
	if let Commands::Memory(memory_args) = &args.command {
		return commands::memory::execute(&config, memory_args).await;
	}

	// Handle the Logs command separately (doesn't need store)
	if let Commands::Logs(logs_args) = &args.command {
		return commands::logs::execute(logs_args).await;
	}

	// Handle the Completion command separately (doesn't need store)
	if let Commands::Completion { shell } = &args.command {
		let mut app = OctocodeArgs::command();
		let name = app.get_name().to_string();
		generate(*shell, &mut app, name, &mut std::io::stdout());
		return Ok(());
	}

	// Initialize the store
	let store = Store::new().await?;
	store.initialize_collections().await?;

	// Execute the appropriate command
	match &args.command {
		Commands::Index(index_args) => {
			commands::index::execute(&store, &config, index_args).await?
		}
		Commands::Search(search_args) => {
			commands::search::execute(&store, search_args, &config).await?
		}
		Commands::View(view_args) => commands::view::execute(&store, view_args, &config).await?,
		Commands::Watch(watch_args) => {
			commands::watch::execute(&store, &config, watch_args).await?
		}
		Commands::GraphRAG(graphrag_args) => {
			commands::graphrag::execute(&store, graphrag_args, &config).await?
		}
		Commands::Clear => commands::clear::execute(&store).await?,
		Commands::Debug(debug_args) => {
			commands::debug::execute(&store, &config, debug_args).await?
		}
		Commands::Config(_) => unreachable!(), // Already handled above
		Commands::Mcp(_) => unreachable!(),    // Already handled above
		Commands::Commit(_) => unreachable!(), // Already handled above
		Commands::Review(_) => unreachable!(), // Already handled above
		Commands::Format(_) => unreachable!(), // Already handled above
		Commands::Logs(_) => unreachable!(),   // Already handled above
		Commands::Memory(_) => unreachable!(), // Already handled above
		Commands::Completion { .. } => unreachable!(), // Already handled above
	}

	Ok(())
}
