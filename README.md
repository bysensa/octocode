# OctoCode - Code Indexer and Search Tool

OctoCode is a smart code indexer and search tool extracted from the OctoDev project. It provides semantic search capabilities across your codebase using embedding models.

## Features

- **Index Codebase**: Recursively index source code files with semantic embeddings
- **Semantic Search**: Search your codebase using natural language queries
- **Multiple Languages**: Support for Rust, Python, JavaScript, TypeScript, Go, PHP, C++, Ruby, JSON, and Bash
- **FastEmbed/Jina Integration**: Uses FastEmbed or Jina AI for generating embeddings
- **File Watching**: Automatically re-index files when they change
- **Configuration**: Flexible configuration system

## Installation

Make sure you have Rust installed, then:

```bash
cargo build --release
```

## Usage

### Basic Commands

```bash
# Index your current directory
cargo run -- index

# Search your codebase
cargo run -- search "function handling HTTP requests"

# Watch for changes and auto-reindex
cargo run -- watch

# View file signatures
cargo run -- view

# Generate default config
cargo run -- config

# Clear the database
cargo run -- clear
```

### Configuration

OctoCode uses a configuration file at `.octocode/config.toml`. Run `cargo run -- config` to generate a default configuration.

Key settings:
- `openrouter.model`: OpenRouter model to use (default: "openai/gpt-4.1-mini")
- `openrouter.api_key`: Your OpenRouter API key (or set `OPENROUTER_API_KEY` env var)
- `embedding_provider`: Choose between "FastEmbed" (default) or "Jina"
- `fastembed.code_model`: Model for code embeddings (default: "all-MiniLM-L6-v2")
- `index.chunk_size`: Size of text chunks for indexing (default: 2000)
- `search.max_results`: Maximum search results to return (default: 50)

### OpenRouter Integration

OctoCode uses OpenRouter for AI functionality. You can use any model available on OpenRouter:

```bash
# Set your API key
export OPENROUTER_API_KEY="your-api-key-here"

# Configure different models
cargo run -- config --model "openai/gpt-4.1-mini"
cargo run -- config --model "anthropic/claude-3.5-sonnet"
cargo run -- config --model "google/gemini-2.5-flash-preview"
```

### Environment Variables

- `OPENROUTER_API_KEY`: Required for OpenRouter API access
- `JINA_API_KEY`: Required if using Jina as embedding provider

## Supported File Types

- **Rust**: .rs
- **Python**: .py
- **JavaScript**: .js
- **TypeScript**: .ts, .tsx, .jsx
- **Go**: .go
- **PHP**: .php
- **C++**: .cpp, .cc, .cxx, .c++, .hpp, .h
- **Ruby**: .rb
- **JSON**: .json
- **Bash**: .sh, .bash
- **Markdown**: .md

## Database

OctoCode uses Lance (columnar database) to store embeddings and metadata. The database is stored in `.octocode/database.lance`.

## Architecture

- **Indexer**: Parses source code using Tree-sitter and generates embeddings
- **Search**: Performs semantic similarity search using vector embeddings
- **Store**: Manages database operations with Lance
- **Languages**: Modular language support for different programming languages

## Contributing

This is an extracted component from the larger OctoDev project. Each language parser is implemented as a separate module in `src/indexer/languages/`.

## License

[License information to be added]