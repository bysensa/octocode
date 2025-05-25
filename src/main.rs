use clap::{Parser, Subcommand};

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

	/// Clear all database tables (useful for debugging)
	Clear,
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

	// Initialize the store
	let store = Store::new().await?;
	store.initialize_collections().await?;

	// Execute the appropriate command
	match &args.command {
		Commands::Index(index_args) => {
			commands::index::execute(&store, &config, index_args).await?
		},
		Commands::Search(search_args) => {
			commands::search::execute(&store, search_args, &config).await?
		},
		Commands::View(view_args) => {
			commands::view::execute(&store, view_args, &config).await?
		},
		Commands::Watch(watch_args) => {
			commands::watch::execute(&store, &config, watch_args).await?
		},
		Commands::GraphRAG(graphrag_args) => {
			commands::graphrag::execute(&store, graphrag_args, &config).await?
		},
		Commands::Clear => {
			commands::clear::execute(&store).await?
		},
		Commands::Config(_) => unreachable!(), // Already handled above
		Commands::Mcp(_) => unreachable!(), // Already handled above
	}

	Ok(())
}
