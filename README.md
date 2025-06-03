# Octocode - Intelligent Code Indexer and Graph Builder

**© 2025 Muvon Un Limited (Hong Kong)** | [Website](https://muvon.io) | [Product Page](https://octocode.muvon.io)

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)

## 🚀 Overview

Octocode is a powerful code indexer and semantic search engine that builds intelligent knowledge graphs of your codebase. It combines advanced AI capabilities with local-first design to provide deep code understanding, relationship mapping, and intelligent assistance for developers.

## ✨ Key Features

### 🔍 **Semantic Code Search**
- Natural language queries across your entire codebase
- Multi-mode search (code, documentation, text, or all)
- Intelligent ranking with similarity scoring
- Symbol expansion for comprehensive results

### 🕸️ **Knowledge Graph (GraphRAG)**
- Automatic relationship discovery between files and modules
- Import/export dependency tracking
- AI-powered file descriptions and architectural insights
- Path finding between code components

### 🌐 **Multi-Language Support**
- **Rust**, **Python**, **JavaScript**, **TypeScript**, **Go**, **PHP**
- **C++**, **Ruby**, **JSON**, **Bash**, **Markdown**
- Tree-sitter based parsing for accurate symbol extraction

### 🧠 **AI-Powered Features**
- Smart commit message generation
- Code review with best practices analysis
- Memory system for storing insights and decisions
- Multiple LLM support via OpenRouter

### 🔌 **MCP Server Integration**
- Built-in Model Context Protocol server
- Seamless integration with AI assistants (Claude Desktop, etc.)
- Real-time file watching and auto-reindexing
- Rich tool ecosystem for code analysis

### ⚡ **Performance & Flexibility**
- Local embedding models (FastEmbed, SentenceTransformer)
- Cloud providers (Jina AI, Voyage AI, Google)
- Lance columnar database for fast vector search
- Incremental indexing and git-aware optimization

## 📦 Installation

### Prerequisites
- **Rust 1.70+** ([install from rustup.rs](https://rustup.rs/))
- **Git** (for repository features)

### Build from Source
```bash
git clone https://github.com/muvon/octocode.git
cd octocode
cargo build --release
```

The binary will be available at `target/release/octocode`.

## 🚀 Quick Start

### 1. Basic Setup
```bash
# Index your current directory
octocode index

# Search your codebase
octocode search "HTTP request handling"

# View code signatures
octocode view "src/**/*.rs"
```

### 2. AI-Powered Git Workflow
```bash
# Generate intelligent commit messages
git add .
octocode commit

# Review code for best practices
octocode review
```

### 3. MCP Server for AI Assistants
```bash
# Start MCP server
octocode mcp

# Use with Claude Desktop or other MCP-compatible tools
# Provides: search_code, search_graphrag, memorize, remember, forget
```

### 4. Advanced Features
```bash
# Enable GraphRAG with AI descriptions
export OPENROUTER_API_KEY="your-key"
octocode config --graphrag-enabled true
octocode index

# Search the knowledge graph
octocode graphrag search --query "authentication modules"

# Watch for changes
octocode watch
```

## 📋 Command Reference

| Command | Description | Example |
|---------|-------------|---------|
| `octocode index` | Index the codebase | `octocode index --reindex` |
| `octocode search <query>` | Semantic code search | `octocode search "error handling"` |
| `octocode graphrag <operation>` | Knowledge graph operations | `octocode graphrag search --query "auth"` |
| `octocode view [pattern]` | View code signatures | `octocode view "src/**/*.rs" --md` |
| `octocode commit` | AI-powered git commit | `octocode commit --all` |
| `octocode review` | Code review assistant | `octocode review --focus security` |
| `octocode mcp` | Start MCP server | `octocode mcp --debug` |
| `octocode watch` | Auto-reindex on changes | `octocode watch --quiet` |
| `octocode config` | Manage configuration | `octocode config --show` |

## 🔧 Configuration

Octocode stores configuration in `~/.local/share/octocode/config.toml`. Quick setup:

```bash
# View current configuration
octocode config --show

# Use local models (no API keys required) 
octocode config \
  --code-embedding-model "fastembed:all-MiniLM-L6-v2" \
  --text-embedding-model "fastembed:multilingual-e5-small"

# Enable GraphRAG with AI descriptions
export OPENROUTER_API_KEY="your-key"
octocode config --graphrag-enabled true

# Set custom OpenRouter model
octocode config --model "openai/gpt-4o-mini"
```

**Default Models:**
- Code embedding: `fastembed:jinaai/jina-embeddings-v2-base-code`
- Text embedding: `fastembed:sentence-transformers/all-MiniLM-L6-v2-quantized`
- LLM: `openai/gpt-4.1-mini` (via OpenRouter)

## 📚 Documentation

- **[Architecture](doc/ARCHITECTURE.md)** - Core components and system design
- **[Configuration](doc/CONFIGURATION.md)** - Setup and configuration options  
- **[Advanced Usage](doc/ADVANCED_USAGE.md)** - Advanced features and workflows
- **[Contributing](doc/CONTRIBUTING.md)** - Development setup and contribution guidelines
- **[Performance](doc/PERFORMANCE.md)** - Performance metrics and optimization tips

## 🔒 Privacy & Security

- **🏠 Local-first**: FastEmbed and SentenceTransformer run entirely offline
- **🔐 No code upload**: Only file metadata sent to AI APIs (when enabled)
- **🔑 Secure storage**: API keys stored locally, environment variables supported
- **📁 Respects .gitignore**: Never indexes sensitive files or directories
- **🛡️ MCP security**: Server runs locally with no external network access for search

## 🌐 Supported Languages

| Language | Extensions | Features |
|----------|------------|----------|
| **Rust** | `.rs` | Full AST parsing, pub/use detection, module structure |
| **Python** | `.py` | Import/class/function extraction, docstring parsing |
| **JavaScript** | `.js`, `.jsx` | ES6 imports/exports, function declarations |
| **TypeScript** | `.ts`, `.tsx` | Type definitions, interface extraction |
| **Go** | `.go` | Package/import analysis, struct/interface parsing |
| **PHP** | `.php` | Class/function extraction, namespace support |
| **C++** | `.cpp`, `.hpp`, `.h` | Include analysis, class/function extraction |
| **Ruby** | `.rb` | Class/module extraction, method definitions |
| **JSON** | `.json` | Structure analysis, key extraction |
| **Bash** | `.sh`, `.bash` | Function and variable extraction |
| **Markdown** | `.md` | Document section indexing, header extraction |

## 🤝 Support & Community

- **🐛 Issues**: [GitHub Issues](https://github.com/muvon/octocode/issues)
- **📧 Email**: [opensource@muvon.io](mailto:opensource@muvon.io)
- **🏢 Company**: Muvon Un Limited (Hong Kong)

## ⚖️ License

This project is licensed under the **Apache License 2.0** - see the [LICENSE](LICENSE) file for details.

---

**Built with ❤️ by the Muvon team in Hong Kong**