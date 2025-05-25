use clap::Args;
use anyhow::Result;
use octocode::config::Config;

#[derive(Args)]
pub struct ConfigArgs {
	/// Set the model to use (e.g., "openai/gpt-4.1-mini", "anthropic/claude-3.5-sonnet")
	#[arg(long)]
	pub model: Option<String>,

	/// Set the embedding model for indexing
	#[arg(long)]
	pub embedding_model: Option<String>,

	/// Set the chunk size for text processing
	#[arg(long)]
	pub chunk_size: Option<usize>,

	/// Set the chunk overlap for text processing
	#[arg(long)]
	pub chunk_overlap: Option<usize>,

	/// Set the maximum number of search results
	#[arg(long)]
	pub max_results: Option<usize>,

	/// Set the similarity threshold for search
	#[arg(long)]
	pub similarity_threshold: Option<f32>,

	/// Set the database path
	#[arg(long)]
	pub database_path: Option<String>,

	/// Enable or disable GraphRAG
	#[arg(long)]
	pub graphrag_enabled: Option<bool>,

	/// Show current configuration
	#[arg(long)]
	pub show: bool,

	/// Reset configuration to defaults
	#[arg(long)]
	pub reset: bool,
}

pub fn execute(args: &ConfigArgs, mut config: Config) -> Result<()> {
	if args.reset {
		config = Config::default();
		config.save()?;
		println!("Configuration reset to defaults");
		return Ok(());
	}

	if args.show {
		println!("Current configuration:");
		println!("Model: {}", config.openrouter.model);
		println!("Embedding model: {}", config.index.embedding_model);
		println!("Chunk size: {}", config.index.chunk_size);
		println!("Chunk overlap: {}", config.index.chunk_overlap);
		println!("Max results: {}", config.search.max_results);
		println!("Similarity threshold: {}", config.search.similarity_threshold);
		println!("Database path: {}", config.database.path);
		println!("GraphRAG enabled: {}", config.index.graphrag_enabled);
		return Ok(());
	}

	let mut updated = false;

	if let Some(model) = &args.model {
		config.openrouter.model = model.clone();
		println!("Model set to: {}", model);
		updated = true;
	}

	if let Some(embedding_model) = &args.embedding_model {
		config.index.embedding_model = embedding_model.clone();
		println!("Embedding model set to: {}", embedding_model);
		updated = true;
	}

	if let Some(chunk_size) = args.chunk_size {
		config.index.chunk_size = chunk_size;
		println!("Chunk size set to: {}", chunk_size);
		updated = true;
	}

	if let Some(chunk_overlap) = args.chunk_overlap {
		config.index.chunk_overlap = chunk_overlap;
		println!("Chunk overlap set to: {}", chunk_overlap);
		updated = true;
	}

	if let Some(max_results) = args.max_results {
		config.search.max_results = max_results;
		println!("Max results set to: {}", max_results);
		updated = true;
	}

	if let Some(similarity_threshold) = args.similarity_threshold {
		config.search.similarity_threshold = similarity_threshold;
		println!("Similarity threshold set to: {}", similarity_threshold);
		updated = true;
	}

	if let Some(database_path) = &args.database_path {
		config.database.path = database_path.clone();
		println!("Database path set to: {}", database_path);
		updated = true;
	}

	if let Some(graphrag_enabled) = args.graphrag_enabled {
		config.index.graphrag_enabled = graphrag_enabled;
		println!("GraphRAG {}", if graphrag_enabled { "enabled" } else { "disabled" });
		updated = true;
	}

	if updated {
		config.save()?;
		println!("Configuration updated successfully!");
	} else {
		println!("No configuration changes made. Use --show to see current settings.");
	}

	Ok(())
}