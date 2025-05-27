use anyhow::Result;
use clap::Args;

use octocode::config::Config;
use octocode::mcp::McpServer;

#[derive(Args, Clone)]
pub struct McpArgs {
	/// Enable debug logging for MCP server
	#[arg(long)]
	pub debug: bool,

	/// Path to the directory to serve (defaults to current directory)
	#[arg(long, default_value = ".")]
	pub path: String,
}

pub async fn run(args: McpArgs) -> Result<()> {
	let config = Config::load()?;

	// Convert path to absolute PathBuf
	let working_directory = std::path::Path::new(&args.path).canonicalize()
		.map_err(|e| anyhow::anyhow!("Invalid path '{}': {}", args.path, e))?;

	// Verify the path exists and is a directory
	if !working_directory.is_dir() {
		return Err(anyhow::anyhow!("Path '{}' is not a directory", working_directory.display()));
	}

	if args.debug {
		eprintln!("MCP Server starting with working directory: {}", working_directory.display());
	}

	let mut server = McpServer::new(config, args.debug, working_directory).await?;
	server.run().await
}
