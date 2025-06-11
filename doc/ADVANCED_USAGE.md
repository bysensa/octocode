# Advanced Usage

## AI-Powered Git Workflow

### Smart Commit Messages

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
```

The AI analyzes your staged changes and creates contextual commit messages following conventional commit format with proper scope and description. For large changes affecting multiple files, it automatically adds detailed bullet points.

**Example output for multi-file changes:**
```
feat(auth): implement OAuth2 authentication

- Add OAuth2 provider configuration
- Implement token validation middleware
- Update user model with OAuth2 fields
- Add comprehensive test coverage
```

### AI-Powered Code Review

```bash
# Review staged changes for best practices and issues
git add .
octocode review

# Review all changes at once
octocode review --all

# Focus on specific areas
octocode review --focus security
octocode review --focus performance
octocode review --focus maintainability

# Filter by severity level
octocode review --severity critical    # Only critical issues
octocode review --severity high        # Critical and high issues
octocode review --severity low         # All issues

# Output in JSON for integration with other tools
octocode review --json
```

**Example review output:**
```
📊 Code Review Summary
═══════════════════════════════════════════════
📁 Files reviewed: 3
🔍 Total issues found: 5
🚨 Critical: 1 | ⚠️  High: 2 | 📝 Medium: 2 | 💡 Low: 0
📈 Overall Score: 75/100

🚨 Issues Found
═══════════════════════════════════════════════
🔥 Hardcoded API Key [CRITICAL]
   Category: Security
   Location: src/api.rs:42-44
   Description: API key hardcoded in source code
   💡 Suggestion: Move to environment variables or config file
```

### AI-Powered Release Management

Octocode provides intelligent release automation with AI-powered version calculation and changelog generation.

```bash
# Preview what would be done (always recommended first)
octocode release --dry-run

# Create a release with AI version calculation
octocode release

# Force a specific version (bypasses AI calculation)
octocode release --force-version "2.0.0"

# Skip confirmation prompt for automation
octocode release --yes

# Use custom changelog file
octocode release --changelog "HISTORY.md"
```

**How it works:**

1. **Project Detection**: Automatically detects project type (Rust, Node.js, PHP, Go)
2. **Version Analysis**: Extracts current version from project files or git tags
3. **Commit Analysis**: Analyzes commits since last release using conventional commit format
4. **AI Calculation**: Uses LLM to determine appropriate semantic version bump
5. **Changelog Generation**: Creates structured changelog with categorized changes
6. **File Updates**: Updates project files (Cargo.toml, package.json, composer.json, VERSION)
7. **Git Operations**: Creates release commit and annotated tag

**Conventional Commits Support:**
- `feat:` → Minor version bump (0.1.0 → 0.2.0)
- `fix:` → Patch version bump (0.1.0 → 0.1.1)
- `BREAKING CHANGE` or `!` → Major version bump (0.1.0 → 1.0.0)
- `chore:`, `docs:`, `style:`, etc. → Patch version bump

**Example workflow:**
```bash
# 1. Make your changes and commit them
git add .
octocode commit

# 2. Preview the release
octocode release --dry-run

# 3. Create the release
octocode release

# 4. Push to remote
git push origin main --tags
```

## MCP Server Integration

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

### Available MCP Tools

| Tool | Description | Parameters |
|------|-------------|------------|
| **search_code** | Semantic code search across the codebase | `query` (string), `mode` (string: all/code/docs/text) |
| **search_graphrag** | Relationship-aware search using GraphRAG | `query` (string) |
| **memorize** | Store important information for future reference | `title` (string), `content` (string), `tags` (array) |
| **remember** | Retrieve stored information by query | `query` (string) |
| **forget** | Remove stored information | `query` (string), `confirm` (boolean) |

### Key Features

- **Intelligent File Watching**: Reindexes code when files change with smart debouncing and ignore pattern support
- **Memory Persistence**: Stores insights across sessions
- **Multi-tool Integration**: Combines search and memory capabilities
- **Debug Mode**: Enhanced logging for troubleshooting and performance monitoring
- **Git Context**: Memory entries automatically tagged with commit info
- **Process Management**: Prevents multiple concurrent indexing operations for optimal performance

## Advanced Search Techniques

### Search Modes

```bash
# Search specific content types
octocode search "database schema" --mode code      # Only code
octocode search "API documentation" --mode docs    # Only docs
octocode search "configuration" --mode text        # Only text files
octocode search "error handling" --mode all        # All content types
```

### Similarity Thresholds

```bash
# High precision search
octocode search "error handling" --threshold 0.8

# Broad results
octocode search "API calls" --threshold 0.3

# Default threshold (0.1)
octocode search "authentication"
```

### Symbol Context Expansion

```bash
# Include related code context
octocode search "user authentication" --expand

# Standard search (no expansion)
octocode search "user authentication"
```

### Output Formats

```bash
# JSON output for programmatic use
octocode search "API endpoints" --json
octocode view "src/**/*.rs" --json

# Markdown for documentation
octocode search "middleware" --md
octocode view "src/**/*.rs" --md
```

## Knowledge Graph Operations

### Basic GraphRAG Commands

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

### Advanced GraphRAG Usage

```bash
# Export graph structure to markdown
octocode graphrag overview --md > project-structure.md

# Search with JSON output for processing
octocode graphrag search --query "authentication" --json

# Get node information in JSON format
octocode graphrag get-node --node-id "src/main.rs" --json
```

## Memory Management

### Through MCP Server

```bash
# Start MCP server to access memory tools
octocode mcp

# Then use through AI assistants:
# - Store architectural decisions
# - Remember bug fixes and their solutions
# - Track feature requirements and implementation notes
# - Maintain development insights across sessions
```

### Memory Types and Organization

The memory system supports different types of information:

- **code**: Code snippets and implementations
- **architecture**: System design decisions
- **bug_fix**: Bug reports and solutions
- **feature**: Feature requirements and specifications
- **documentation**: Important documentation notes
- **user_preference**: User-specific preferences
- **decision**: Project decisions and rationale
- **learning**: Insights and lessons learned
- **configuration**: Setup and configuration notes
- **testing**: Test strategies and results
- **performance**: Performance optimizations
- **security**: Security considerations
- **insight**: General insights and observations

## Custom Model Configuration

### Using Different Models for Different Tasks

```bash
# Use Claude for better code understanding
octocode config --model "anthropic/claude-3.5-sonnet"

# Use local models via OpenRouter
octocode config --model "local/llama-3.2-70b"
```

### Per-Task Model Configuration

```toml
[graphrag]
description_model = "openai/gpt-4o"
relationship_model = "anthropic/claude-3.5-sonnet"

[openrouter]
model = "openai/gpt-4o-mini"  # Default for other tasks
```

## File Signature Analysis

### Viewing Code Structure

```bash
# View code signatures in current directory
octocode view

# View specific files with glob patterns
octocode view "src/**/*.rs"
octocode view "**/*.py"
octocode view "src/auth/*.ts"

# Output formats
octocode view --json                    # JSON format
octocode view --md                      # Markdown format
octocode view "src/**/*.rs" --md        # Specific files in markdown
```

### Use Cases for Signature Analysis

- **Code Review**: Understand structure before detailed review
- **Documentation**: Generate API documentation
- **Refactoring**: Identify patterns and dependencies
- **Onboarding**: Help new developers understand codebase structure

## Real-time Monitoring

### Watch Mode

```bash
# Watch for changes and auto-reindex
octocode watch

# Watch with custom debounce time (1-30 seconds, default: 2)
octocode watch --debounce 5

# Watch with custom additional delay (0-5000ms, default: 1000ms)
octocode watch --additional-delay 2000

# Combine both timing options
octocode watch --debounce 3 --additional-delay 1500

# Watch in quiet mode (less output)
octocode watch --quiet

# Watch without git requirements
octocode watch --no-git
```

### Enhanced File Filtering

The watch mode now properly respects ignore patterns from:
- `.gitignore` - Standard Git ignore patterns
- `.noindex` - Custom ignore patterns for indexing

**Supported ignore patterns:**
- Exact matches: `file.txt`
- Directory patterns: `directory/`
- Wildcard patterns: `*.log`, `temp*`
- File extensions: `*.tmp`

**Default ignored paths:**
- `.octocode/`, `target/`, `.git/`
- `node_modules/`, `.vscode/`, `.idea/`
- `.DS_Store`, `Thumbs.db`, `.tmp`, `.temp`

### Performance Optimizations

The watch mode includes several performance improvements:
- **Debouncing**: Prevents rapid re-indexing on multiple file changes
- **Smart filtering**: Early filtering of irrelevant file events
- **Process management**: Prevents multiple concurrent indexing operations

### Integration with Development Workflow

```bash
# Start watching in background with optimal settings
octocode watch --quiet --debounce 2 --additional-delay 1000 &

# For development with frequent changes (faster response)
octocode watch --debounce 1 --additional-delay 500

# For large projects (conservative settings)
octocode watch --debounce 5 --additional-delay 2000

# Continue development...
# Files are automatically reindexed as you work

# Stop watching
pkill -f "octocode watch"
```

## Batch Operations and Automation

### Scripting Examples

```bash
#!/bin/bash
# Complete reindex script
octocode clear
octocode index --reindex
octocode mcp &
echo "Octocode ready for development"
```

```bash
#!/bin/bash
# Daily maintenance script
octocode index --reindex
octocode graphrag overview --md > docs/project-structure.md
octocode view "src/**/*.rs" --md > docs/api-reference.md
```

### CI/CD Integration

```yaml
# GitHub Actions example
- name: Generate Code Documentation
  run: |
    cargo build --release
    ./target/release/octocode index
    ./target/release/octocode view "src/**/*.rs" --md > docs/api.md
    ./target/release/octocode graphrag overview --md > docs/structure.md
```

## Debugging and Troubleshooting

### Debug Commands

```bash
# List all indexed files
octocode debug --list-files

# Check configuration
octocode config --show

# Clear all data and start fresh
octocode clear

# Reindex with verbose output
octocode index --reindex
```

### MCP Server Debugging

```bash
# Start MCP server with debug logging
octocode mcp --debug

# Check server status and file watcher behavior
octocode mcp --debug --path /path/to/project
```

**Debug output includes:**
- File watcher startup and ignore pattern loading
- Debouncing events and timing information
- Process spawning and completion status
- Indexing performance metrics

### Common Issues and Solutions

1. **Slow indexing**: Reduce chunk size or use faster embedding models
2. **Poor search results**: Adjust similarity threshold or try different embedding models
3. **Memory issues**: Reduce max_memories or clear old data
4. **Git integration not working**: Ensure you're in a git repository and have staged changes

## Performance Optimization

### For Large Codebases

```toml
[index]
chunk_size = 1000        # Smaller chunks for faster processing
embeddings_batch_size = 64  # Larger batches for better throughput

[search]
max_results = 20         # Limit results for faster response
similarity_threshold = 0.2  # Higher threshold for more relevant results

[memory]
max_memories = 50000     # Increase for large projects
```

### Memory Usage Optimization

```bash
# Clear old data periodically
octocode clear

# Use local embedding models to reduce API calls
octocode config --code-embedding-model "fastembed:all-MiniLM-L6-v2"

# Limit search results
octocode config --max-results 20
```
