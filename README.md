# Octocode - Intelligent Code Indexergand Graph Builder

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
- **Multiple Embedding Providers**: FastEmbed (local), SentenceTransformer (local), Jina AI, Voyage AI, or Google (cloud)
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
# View current configuration
octocode config --show

# Set embedding models (provider auto-detected from model string)
octocode config \
  --code-embedding-model "sentencetransformer:microsoft/codebert-base" \
  --text-embedding-model "sentencetransformer:sentence-transformers/all-mpnet-base-v2"

# Index your current directory
octocode index

# Search your codebase
octocode search "HTTP request handling"

# Enable GraphRAG for relationship building
echo 'OPENROUTER_API_KEY="your-key-here"' > .env
octocode config --graphrag-enabled true

# Rebuild index with GraphRAG
octocode index

# Search the knowledge graph
octocode graphrag search --query "authentication modules"
```

## ⚙️ Configuration

Octocode stores configuration in `~/.local/share/octocode/config.toml`. View current settings with:

```bash
octocode config --show
```

### Embedding Configuration

#### **Quick Setup Examples**

```bash
# Use SentenceTransformer (local, no API key needed)
octocode config \
  --code-embedding-model "sentencetransformer:microsoft/codebert-base" \
  --text-embedding-model "sentencetransformer:sentence-transformers/all-mpnet-base-v2"

# Use FastEmbed (local, no API key needed)
octocode config \
  --code-embedding-model "fastembed:all-MiniLM-L6-v2" \
  --text-embedding-model "fastembed:multilingual-e5-small"

# Mix providers as needed
octocode config \
  --code-embedding-model "sentencetransformer:microsoft/codebert-base" \
  --text-embedding-model "fastembed:multilingual-e5-small"

# Use cloud providers (API keys required via environment variables)
octocode config \
  --code-embedding-model "jinaai:jina-embeddings-v2-base-code" \
  --text-embedding-model "voyageai:voyage-3"
```

### Configuration File Structure

```toml
[openrouter]
model = "openai/gpt-4o-mini"
api_key = "your-openrouter-key"  # Or set OPENROUTER_API_KEY env var

[embedding]
# Direct model configuration - provider auto-detected from prefix
code_model = "sentencetransformer:microsoft/codebert-base"
text_model = "sentencetransformer:sentence-transformers/all-mpnet-base-v2"

# Provider-specific sections only for API keys
[embedding.jina]
api_key = "your-jina-key"  # Or set JINA_API_KEY env var

[embedding.voyage]
api_key = "your-voyage-key"  # Or set VOYAGE_API_KEY env var

[embedding.google]
api_key = "your-google-key"  # Or set GOOGLE_API_KEY env var

[graphrag]
enabled = true
description_model = "openai/gpt-4o-mini"
relationship_model = "openai/gpt-4o-mini"

[search]
max_results = 50
similarity_threshold = 0.1

[index]
chunk_size = 2000
graphrag_enabled = true
```

### Supported Embedding Providers

| Provider | Format | API Key Required | Local/Cloud |
|----------|--------|------------------|-------------|
| **SentenceTransformer** | `sentencetransformer:model-name` | ❌ No | 🖥️ Local |
| **FastEmbed** | `fastembed:model-name` | ❌ No | 🖥️ Local |
| **Jina AI** | `jinaai:model-name` | ✅ Yes | ☁️ Cloud |
| **Voyage AI** | `voyageai:model-name` | ✅ Yes | ☁️ Cloud |
| **Google** | `google:model-name` | ✅ Yes | ☁️ Cloud |

### Popular Model Recommendations

#### **For Code (code_model)**
```bash
# Best for code understanding
sentencetransformer:microsoft/codebert-base        # 768 dim, excellent for code
sentencetransformer:microsoft/unixcoder-base       # 768 dim, Unix/shell code

# Fast local alternatives
fastembed:all-MiniLM-L6-v2                        # 384 dim, fast and efficient

# Cloud options (API key required)
jinaai:jina-embeddings-v2-base-code               # 768 dim, specialized for code
voyageai:voyage-code-2                            # 1536 dim, high quality
```

#### **For Text/Documentation (text_model)**
```bash
# Best general purpose
sentencetransformer:sentence-transformers/all-mpnet-base-v2  # 768 dim, excellent quality
sentencetransformer:BAAI/bge-base-en-v1.5                   # 768 dim, high performance

# Fast alternatives
fastembed:multilingual-e5-small                    # 384 dim, supports multiple languages
sentencetransformer:sentence-transformers/all-MiniLM-L6-v2  # 384 dim, fast

# High-quality cloud options
jinaai:jina-embeddings-v3                         # 1024 dim, latest Jina model
voyageai:voyage-3                                 # 1024 dim, excellent for text
```

### Environment Variables
```bash
export OPENROUTER_API_KEY="your-openrouter-api-key"
export JINA_API_KEY="your-jina-key"         # If using Jina AI models
export VOYAGE_API_KEY="your-voyage-key"     # If using Voyage AI models
export GOOGLE_API_KEY="your-google-key"     # If using Google models
```

**Note**: Environment variables always take priority over config file settings.

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
