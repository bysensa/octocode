# Octocode - Intelligent Code Indexer and Graph Builder

**© 2025 Muvon Un Limited (Hong Kong)** | Contact: [opensource@muvon.io](mailto:opensource@muvon.io) | Website: [muvon.io](https://muvon.io) | Product: [octocode.muvon.io](https://octocode.muvon.io)

---

Octocode is a smart code indexer and semantic search tool that builds intelligent knowledge graphs of your codebase. It provides powerful semantic search capabilities across multiple programming languages, creates file-level relationship graphs for better code understanding, includes an MCP (Model Context Protocol) server for seamless AI integration, and features AI-powered memory management and git commit assistance.

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

### **MCP Server Integration**
- **Model Context Protocol Server**: Built-in MCP server for seamless AI tool integration
- **AI Assistant Tools**: `search_code`, `search_graphrag`, `memorize`, `remember`, and `forget` tools
- **Real-time File Watching**: Automatic reindexing when code changes are detected
- **Memory System**: Persistent storage for important insights and context

### **Memory Management System**
- **AI-Powered Memory**: Store and retrieve important insights, decisions, and context
- **Semantic Memory Search**: Find stored information using natural language queries
- **Git Integration**: Automatic tagging with commit hashes and file relationships
- **Memory Types**: Support for code, architecture, bug fixes, features, decisions, and notes
- **Relationship Tracking**: Automatic discovery of related memories

### **AI-Powered Git Integration**
- **Smart Commit Messages**: Generate intelligent commit messages using AI
- **Staged Changes Analysis**: Automatic analysis of your staged changes
- **Multiple LLM Support**: Works with any OpenRouter-compatible model
- **Interactive Workflow**: Review and edit generated messages before committing

### **Advanced Features**
- **Real-time Watch Mode**: Auto-reindex when files change
- **Multiple Embedding Providers**: FastEmbed (local), SentenceTransformer (local), Jina AI, Voyage AI, or Google (cloud)
- **OpenRouter Integration**: Use any LLM model for AI features
- **Fast Vector Database**: Lance columnar database for efficient storage
- **Gitignore Respect**: Only indexes files that should be tracked
- **File Signature Analysis**: Extract and view function/method signatures from code
- **Debug Tools**: Built-in debugging commands for troubleshooting

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

# Generate AI commit message
git add .
octocode commit

# Enable GraphRAG for relationship building
echo 'OPENROUTER_API_KEY="your-key-here"' > .env
octocode config --graphrag-enabled true

# Rebuild index with GraphRAG
octocode index

# Search the knowledge graph
octocode graphrag search --query "authentication modules"

# Start MCP server for AI assistant integration
octocode mcp

# Clear index and start fresh
octocode clear
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

[memory]
enabled = true
max_memories = 10000
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

# Search with different modes and thresholds
octocode search "API endpoints" --mode code --threshold 0.7
octocode search "documentation" --mode docs --threshold 0.5
octocode search "configuration files" --mode text --threshold 0.3
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

### Memory Management
```bash
# Store important information
octocode mcp  # Then use memorize tool via AI assistant

# Or use memory through MCP tools:
# - memorize: Store insights, decisions, and context
# - remember: Search stored memories
# - forget: Remove outdated information
```

### AI-Powered Git Workflow
```bash
# Add changes and generate commit message
git add .
octocode commit

# Add all changes in one command
octocode commit --all

# Custom commit message (skips AI generation)
octocode commit --message "fix: resolve authentication bug"

# Skip confirmation prompt
octocode commit --all --yes
```

### MCP Server for AI Assistants
```bash
# Start MCP server (default: current directory)
octocode mcp

# Start with debug logging
octocode mcp --debug

# Serve a specific directory
octocode mcp --path /path/to/project

# Use with AI assistants that support MCP (Claude Desktop, etc.)
# The server provides these tools:
# - search_code: Semantic code search
# - search_graphrag: Relationship-aware search  
# - memorize: Store important information
# - remember: Retrieve stored information
# - forget: Remove stored information
```

### File Signatures and Structure
```bash
# View code signatures in current directory
octocode view

# View specific files with glob patterns
octocode view "src/**/*.rs"

# Output in JSON format
octocode view --json

# Output in Markdown format (great for AI analysis)
octocode view --md

# View specific file patterns with markdown output
octocode view "src/**/*.rs" --md
```

### Real-time Monitoring
```bash
# Watch for changes and auto-reindex
octocode watch

# Watch with custom debounce time
octocode watch --debounce 5

# Watch in quiet mode
octocode watch --quiet
```

### Database Management
```bash
# Clear all data and start fresh
octocode clear

# Reindex everything from scratch
octocode index --reindex

# Skip git requirements for non-git projects
octocode index --no-git
```

### Debug and Troubleshooting
```bash
# List all indexed files
octocode debug --list-files

# Clear all data and start fresh
octocode clear
```

## 🏗️ Architecture

### Core Components

1. **Indexer Engine**: Multi-language code parser using Tree-sitter
2. **Embedding System**: FastEmbed, SentenceTransformer, or cloud providers for semantic vectors
3. **Vector Database**: Lance columnar database for fast similarity search
4. **GraphRAG Builder**: AI-powered file relationship extraction
5. **Search Engine**: Semantic similarity with keyword boosting
6. **MCP Server**: Model Context Protocol server for AI assistant integration
7. **Memory System**: Persistent storage for insights and contextual information
8. **Git Integration**: Smart commit message generation and change tracking

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

## 🤖 MCP Server Integration

Octocode includes a built-in Model Context Protocol (MCP) server that provides AI assistants with powerful tools for code analysis and memory management.

### Available MCP Tools

| Tool | Description | Parameters |
|------|-------------|------------|
| **search_code** | Semantic code search across the codebase | `query` (string), `mode` (string: all/code/docs/text) |
| **search_graphrag** | Relationship-aware search using GraphRAG | `query` (string) |
| **memorize** | Store important information for future reference | `title` (string), `content` (string), `tags` (array) |
| **remember** | Retrieve stored information by query | `query` (string) |
| **forget** | Remove stored information | `query` (string), `confirm` (boolean) |

### Setting Up MCP Server

1. **Start the server:**
   ```bash
   octocode mcp --path /path/to/your/project
   ```

2. **Configure in Claude Desktop** (add to config):
   ```json
   {
     "mcpServers": {
       "octocode": {
         "command": "octocode",
         "args": ["mcp", "--path", "/path/to/your/project"]
       }
     }
   }
   ```

3. **Use with other MCP-compatible AI assistants** by configuring the server endpoint

### Key Features

- **Automatic File Watching**: Reindexes code when files change
- **Memory Persistence**: Stores insights across sessions
- **Multi-tool Integration**: Combines search and memory capabilities
- **Debug Mode**: Enhanced logging for troubleshooting
- **Git Context**: Memory entries automatically tagged with commit info

## 🔧 Advanced Usage

### AI-Powered Git Workflow
```bash
# Basic usage - analyze staged changes and generate commit message
git add .
octocode commit

# Add all changes and commit in one step
octocode commit --all

# Provide extra context to help AI generate better commit message
octocode commit --message "Refactoring the authentication system to support OAuth2"

# Auto-commit without confirmation
octocode commit --all --yes

# The AI analyzes your staged changes and creates contextual commit messages
# following conventional commit format with proper scope and description
# For large changes affecting multiple files, it automatically adds detailed bullet points

# Example output for multi-file changes:
# feat(auth): implement OAuth2 authentication
# 
# - Add OAuth2 provider configuration
# - Implement token validation middleware
# - Update user model with OAuth2 fields
# - Add comprehensive test coverage
```

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

### Memory Management through MCP
```bash
# Start MCP server to access memory tools
octocode mcp

# Then use through AI assistants:
# - Store architectural decisions
# - Remember bug fixes and their solutions
# - Track feature requirements and implementation notes
# - Maintain development insights across sessions
```

### Advanced Search Options
```bash
# Search with specific similarity thresholds
octocode search "error handling" --threshold 0.8  # High precision
octocode search "API calls" --threshold 0.3       # Broad results

# Search specific content types
octocode search "database schema" --mode code      # Only code
octocode search "API documentation" --mode docs    # Only docs
octocode search "configuration" --mode text        # Only text files

# Expand symbol context
octocode search "user authentication" --expand     # Include related code

# Output formats for integration
octocode search "JWT tokens" --json               # JSON output
octocode search "middleware" --md                 # Markdown output
```
### Batch Operations
```bash
# Clear and rebuild entire index
octocode clear && octocode index

# Force reindex all files
octocode index --reindex

# Start MCP server after indexing
octocode index && octocode mcp
```

### Output Formats
```bash
# JSON output for programmatic use
octocode search "API endpoints" --json
octocode view "src/**/*.rs" --json

# Markdown for documentation
octocode graphrag overview --md > project-structure.md
octocode view "src/**/*.rs" --md > code-signatures.md
```

## 📋 Command Reference

| Command | Description | Key Options |
|---------|-------------|-------------|
| `octocode index` | Index the codebase | `--reindex`, `--no-git` |
| `octocode search <query>` | Search code semantically | `--json`, `--md`, `--expand`, `--mode`, `--threshold` |
| `octocode graphrag <operation>` | GraphRAG operations | `--query`, `--node-id`, `--json`, `--md` |
| `octocode view [files]` | View file signatures | `--json`, `--md` |
| `octocode watch` | Auto-reindex on changes | `--quiet`, `--debounce`, `--no-git` |
| `octocode config` | Manage configuration | `--show`, `--model`, `--graphrag-enabled` |
| `octocode mcp` | Start MCP server | `--debug`, `--path` |
| `octocode commit` | AI-powered git commit | `--all`, `--message`, `--yes` |
| `octocode clear` | Clear all data | |
| `octocode debug` | Debug and troubleshooting | `--list-files` |

### GraphRAG Operations
- `search`: Search nodes by semantic query
- `get-node`: Get detailed node information  
- `get-relationships`: Get node relationships
- `find-path`: Find paths between nodes
- `overview`: Get graph structure overview

## 🔒 Privacy & Security

- **Local-First**: FastEmbed and SentenceTransformer run entirely offline
- **API Security**: OpenRouter/Jina keys stored locally only
- **No Code Upload**: Only file metadata and descriptions sent to AI APIs
- **Gitignore Respect**: Never indexes sensitive files like `.env`, `secrets/`
- **MCP Server Security**: Runs locally, no external network access for search operations

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
