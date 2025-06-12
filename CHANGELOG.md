# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2025-06-12

### ✨ Features

- add mode option to selectively clear tables
- add multi-query search usage and support details
- add hierarchical bottom-up chunking for docs
- add show-file option to display file chunks
- add --no-verify flag to skip git hooks
- add GraphRAG data cleanup on file removal
- improve UTF-8 slicing and path handling; build from D...
- build GraphRAG from existing DB if enabled
- add detailed multi-mode search with markdown output

### 🐛 Bug Fixes

- preserve formatting when updating version fields
- merge tiny chunks to reduce excessive chunk creation
- add optional context field to data schema
- update default model names and versions
- suppress MCP server logs during graph loading
- properly handle .noindex ignore files
- remove unnecessary timeouts on memory ops
- update Rust version and copy config templates
- require curl and update repo URLs to Muvon/octocode
- fix variable interpolation in release workflow URLs

### 🔧 Other Changes

- docs: replace "reindex" with "index" for accuracy in docs
- refactor: centralize search embeddings generation logic
- docs: add AI-powered release management docs and CLI usage
- refactor: unify GraphRAG config under graphrag section
- refactor: use shared HTTP client with pooling
- chore: update Apache License text to latest version
- chore: add Rust formatting and linting hooks
- refactor: move git file detection to utils module and clean code
