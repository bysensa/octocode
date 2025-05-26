# Octocode - Intelligent Code Indexer and Graph Builder

**© 2025 Muvon Un Limited (Hong Kong)** | Contact: [opensource@muvon.io](mailto:opensource@muvon.io) | Website: [muvon.io](https://muvon.io) | Product: [octocode.muvon.io](https://octocode.muvon.io)

---

Octocode is a smart code indexer and semantic search tool that builds intelligent knowledge graphs of your codebase. It provides powerful semantic search capabilities across multiple programming languages and creates file-level relationship graphs for better code understanding.

## 🚀 Key Features

### **Semantic Code Search**
- **Natural Language Queries**: Search your codebase using plain English
- **Multi-Language Support**: Rust, Python, JavaScript, TypeScript, Go, PHP, C++, Ruby, JSON, Bash, and Markdown
- **Smart Indexing**: Automatic parsing and semantic understanding of code structure

### **Knowledge Graph Generation (GraphRAG)**
- **File-Level Relationships**: Understand how your modules and files connect
- **Import/Export Tracking**: Automatic detection of dependencies between files
- **Module Hierarchy**: Visualize parent-child and sibling module relationships
- **AI-Powered Descriptions**: Intelligent summaries of what each file does

### **Advanced Features**
- **Real-time Watch Mode**: Auto-reindex when files change
- **Multiple Embedding Providers**: FastEmbed (local) or Jina AI (cloud)
- **OpenRouter Integration**: Use any LLM model for AI features
- **Fast Vector Database**: Lance columnar database for efficient storage
- **Gitignore Respect**: Only indexes files that should be tracked

## 📦 Installation

### Prerequisites
- **Rust 1.70+** (install from [rustup.rs](https://rustup.rs/))

### Build from Source
```bash
git clone https://github.com/muvon/octocode.git
cd octocode
cargo build --release
```

The binary will be available at `target/release/octocode`.

### Quick Start
```bash
# Generate default configuration
./target/release/octocode config

# Index your current directory
./target/release/octocode index

# Search your codebase
./target/release/octocode search "HTTP request handling"

# Enable GraphRAG for relationship building
echo 'graphrag.enabled = true' >> .octocode/config.toml
echo 'OPENROUTER_API_KEY="your-key-here"' > .env

# Rebuild index with GraphRAG
./target/release/octocode index

# Search the knowledge graph
./target/release/octocode graphrag search --query "authentication modules"
```

## ⚙️ Configuration

Octocode uses `.octocode/config.toml` for configuration. Generate it with:

```bash
octocode config
```

### Key Configuration Options

```toml
[graphrag]
enabled = true  # Enable file-level relationship graphs
description_model = "openai/gpt-4o-mini"  # Model for file descriptions
relationship_model = "openai/gpt-4o-mini"  # Model for relationship detection

[openrouter]
model = "openai/gpt-4o-mini"  # Default model for AI features
api_key = "your-openrouter-key"  # Or set OPENROUTER_API_KEY env var

[embedding_provider]
provider = "FastEmbed"  # or "Jina"

[fastembed]
code_model = "all-MiniLM-L6-v2"  # Local embedding model

[search]
max_results = 50
similarity_threshold = 0.1

[index]
chunk_size = 2000
graphrag_enabled = true  # Same as graphrag.enabled
```

### Environment Variables
```bash
export OPENROUTER_API_KEY="your-openrouter-api-key"
export JINA_API_KEY="your-jina-key"  # If using Jina embeddings
```

## 🔍 Usage Examples

### Basic Code Search
```bash
# Find HTTP-related code
octocode search "HTTP client requests"

# Find error handling patterns
octocode search "error handling and exceptions"

# Search for specific functions
octocode search "authentication middleware"
```

### Knowledge Graph Operations
```bash
# Search the relationship graph
octocode graphrag search --query "database models"

# Get detailed information about a file
octocode graphrag get-node --node-id "src/auth/mod.rs"

# Find relationships for a specific file
octocode graphrag get-relationships --node-id "src/auth/mod.rs"

# Find connections between two modules
octocode graphrag find-path --source-id "src/auth/mod.rs" --target-id "src/database/mod.rs"

# Get graph overview
octocode graphrag overview
```

### File Signatures and Structure
```bash
# View code signatures in current directory
octocode view

# Output in JSON format
octocode view --json

# Output in Markdown format
octocode view --md
```

### Real-time Monitoring
```bash
# Watch for changes and auto-reindex
octocode watch
```

## 🏗️ Architecture

### Core Components

1. **Indexer Engine**: Multi-language code parser using Tree-sitter
2. **Embedding System**: FastEmbed or Jina AI for semantic vectors
3. **Vector Database**: Lance columnar database for fast similarity search
4. **GraphRAG Builder**: AI-powered file relationship extraction
5. **Search Engine**: Semantic similarity with keyword boosting

### Knowledge Graph Structure

**Nodes** represent files/modules with:
- File path and metadata
- AI-generated descriptions
- Extracted symbols (functions, classes, etc.)
- Import/export lists
- Vector embeddings

**Relationships** represent connections:
- `imports`: Direct import dependencies
- `sibling_module`: Files in same directory
- `parent_module` / `child_module`: Hierarchical relationships

## 🌐 Supported Languages

| Language    | Extensions            | Features                    |
|-------------|----------------------|----------------------------|
| **Rust**    | `.rs`                | Full AST parsing, pub/use detection |
| **Python**  | `.py`                | Import/class/function extraction |
| **JavaScript** | `.js`, `.jsx`     | ES6 imports/exports, functions |
| **TypeScript** | `.ts`, `.tsx`     | Type definitions, modules |
| **Go**      | `.go`                | Package/import analysis |
| **PHP**     | `.php`               | Class/function extraction |
| **C++**     | `.cpp`, `.hpp`, `.h` | Include analysis |
| **Ruby**    | `.rb`                | Class/module extraction |
| **JSON**    | `.json`              | Structure analysis |
| **Bash**    | `.sh`, `.bash`       | Function and variable extraction |
| **Markdown** | `.md`               | Document section indexing |

## 🔧 Advanced Usage

### Custom Models
Use any OpenRouter-compatible model:

```bash
# Use Claude for better code understanding
octocode config --model "anthropic/claude-3.5-sonnet"

# Use local models via OpenRouter
octocode config --model "local/llama-3.2-70b"

# Different models for different tasks
echo '[graphrag]
description_model = "openai/gpt-4o"
relationship_model = "anthropic/claude-3.5-sonnet"' >> .octocode/config.toml
```

### Batch Operations
```bash
# Clear and rebuild entire index
octocode clear && octocode index

# Force reindex all files
octocode index --force
```

### Output Formats
```bash
# JSON output for programmatic use
octocode search "API endpoints" --json

# Markdown for documentation
octocode graphrag overview --md > project-structure.md
```

## 🔒 Privacy & Security

- **Local-First**: FastEmbed runs entirely offline
- **API Security**: OpenRouter/Jina keys stored locally only
- **No Code Upload**: Only file metadata and descriptions sent to AI APIs
- **Gitignore Respect**: Never indexes sensitive files like `.env`, `secrets/`

## 📊 Performance

### Typical Performance Metrics
- **Indexing Speed**: ~100-500 files/second (depending on file size)
- **Search Latency**: <100ms for most queries
- **Memory Usage**: ~50MB base + ~1KB per indexed file
- **Storage**: ~10KB per file in Lance database

### Optimization Tips
```toml
[index]
chunk_size = 1000        # Smaller chunks for faster indexing
embeddings_batch_size = 64  # Larger batches for better throughput

[search]
max_results = 20         # Limit results for faster response
```

## 🤝 Contributing

We welcome contributions! This project is part of the larger Muvon ecosystem.

### Development Setup
```bash
git clone https://github.com/muvon/octocode.git
cd octocode
cargo build
cargo test
```

### Adding Language Support
Language parsers are in `src/indexer/languages/`. Each language needs:
1. Tree-sitter grammar dependency
2. Language implementation in `src/indexer/languages/your_lang.rs`
3. Registration in `src/indexer/languages/mod.rs`

## 📞 Support & Contact

- **Issues**: [GitHub Issues](https://github.com/muvon/octocode/issues)
- **Email**: [opensource@muvon.io](mailto:opensource@muvon.io)
- **Company**: Muvon Un Limited (Hong Kong)
- **Website**: [muvon.io](https://muvon.io)
- **Product Page**: [octocode.muvon.io](https://octocode.muvon.io)

## ⚖️ License

Copyright © 2025 Muvon Un Limited. All rights reserved.

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

**Built with ❤️ by the Muvon team in Hong Kong**