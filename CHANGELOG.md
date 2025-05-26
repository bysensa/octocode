# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Comprehensive build system with Makefile
- GitHub Actions CI/CD workflows for automated testing and releases
- Cross-platform static binary compilation support
- Docker containerization with multi-stage builds
- Automated dependency updates workflow

### Changed
- Optimized Cargo.toml for static linking and smaller binaries
- Enhanced release profile with size optimizations

### Security
- Added automated security audits in CI pipeline

## [0.1.0] - 2024-XX-XX

### Added
- Initial release of octocode
- Intelligent code indexer and graph builder
- Semantic search capabilities
- Support for multiple programming languages:
  - PHP
  - Rust
  - Python
  - JavaScript/TypeScript
  - JSON
  - Go
  - C++
  - Bash
  - Ruby
- Vector database integration with LanceDB
- Real-time file watching and indexing
- Command-line interface with clap

### Features
- Fast and efficient code indexing
- Semantic search using embeddings
- Language-aware parsing with tree-sitter
- Configurable via TOML files
- Cross-platform support (Linux, macOS, Windows)