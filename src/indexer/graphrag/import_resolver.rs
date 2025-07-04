// Copyright 2025 Muvon Un Limited
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Import resolution engine for GraphRAG relationship building
//!
//! This module provides language-specific import resolution to map import statements
//! to actual file paths, enabling semantic relationship discovery.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Core import resolver that maps import paths to actual files
pub struct ImportResolver {
	/// Cache of resolved imports to avoid repeated file system operations
	resolution_cache: HashMap<String, Option<String>>,
	/// Map of all available files by language for quick lookup
	file_registry: HashMap<String, Vec<String>>,
}

impl ImportResolver {
	/// Create a new import resolver with file registry
	pub fn new(all_files: &[String]) -> Self {
		let mut file_registry = HashMap::new();

		// Group files by language for efficient lookup
		for file_path in all_files {
			if let Some(language) = detect_language_from_path(file_path) {
				file_registry
					.entry(language)
					.or_insert_with(Vec::new)
					.push(file_path.clone());
			}
		}

		Self {
			resolution_cache: HashMap::new(),
			file_registry,
		}
	}

	/// Resolve an import statement to an actual file path
	pub fn resolve_import(
		&mut self,
		import_path: &str,
		source_file: &str,
		language: &str,
	) -> Option<String> {
		// Create cache key
		let cache_key = format!("{}:{}:{}", language, source_file, import_path);

		// Check cache first
		if let Some(cached_result) = self.resolution_cache.get(&cache_key) {
			return cached_result.clone();
		}

		// Resolve based on language
		let resolved = match language {
			"rust" => self.resolve_rust_import(import_path, source_file),
			"javascript" | "typescript" => self.resolve_js_import(import_path, source_file),
			"python" => self.resolve_python_import(import_path, source_file),
			"go" => self.resolve_go_import(import_path, source_file),
			"php" => self.resolve_php_import(import_path, source_file),
			"cpp" | "c" => self.resolve_cpp_import(import_path, source_file),
			"ruby" => self.resolve_ruby_import(import_path, source_file),
			"bash" => self.resolve_bash_import(import_path, source_file),
			_ => None,
		};

		// Cache the result
		self.resolution_cache.insert(cache_key, resolved.clone());
		resolved
	}

	/// Resolve Rust use statements to file paths
	fn resolve_rust_import(&self, import_path: &str, source_file: &str) -> Option<String> {
		let rust_files = self.file_registry.get("rust")?;
		let source_path = Path::new(source_file);
		let source_dir = source_path.parent()?;

		// Handle different Rust import patterns
		if import_path.starts_with("crate::") {
			// Absolute crate path: crate::module::Item
			let module_path = import_path.strip_prefix("crate::")?;
			self.resolve_rust_module_path(module_path, source_file, rust_files)
		} else if import_path.starts_with("super::") {
			// Parent module: super::module::Item
			let module_path = import_path.strip_prefix("super::")?;
			let parent_dir = source_dir.parent()?;
			self.resolve_rust_relative_path(module_path, parent_dir, rust_files)
		} else if import_path.starts_with("self::") {
			// Current module: self::module::Item
			let module_path = import_path.strip_prefix("self::")?;
			self.resolve_rust_relative_path(module_path, source_dir, rust_files)
		} else if import_path.contains("::") {
			// External crate or absolute path
			self.resolve_rust_module_path(import_path, source_file, rust_files)
		} else {
			// Simple import - look for file in same directory
			self.find_rust_file_in_dir(&format!("{}.rs", import_path), source_dir, rust_files)
		}
	}

	/// Resolve JavaScript/TypeScript imports to file paths
	fn resolve_js_import(&self, import_path: &str, source_file: &str) -> Option<String> {
		let js_files = self
			.file_registry
			.get("javascript")
			.or_else(|| self.file_registry.get("typescript"))?;
		let source_path = Path::new(source_file);
		let source_dir = source_path.parent()?;

		if import_path.starts_with("./") || import_path.starts_with("../") {
			// Relative import
			let relative_path = Path::new(import_path);
			let resolved_path = source_dir.join(relative_path);
			self.find_js_file_with_extensions(&resolved_path, js_files)
		} else if import_path.starts_with('/') {
			// Absolute import from project root
			self.find_js_file_with_extensions(Path::new(import_path), js_files)
		} else {
			// Module import - look in node_modules or as relative
			// For now, treat as relative if no node_modules handling
			let module_path_str = format!("./{}", import_path);
			let module_path = Path::new(&module_path_str);
			let resolved_path = source_dir.join(module_path);
			self.find_js_file_with_extensions(&resolved_path, js_files)
		}
	}

	/// Resolve Python imports to file paths
	fn resolve_python_import(&self, import_path: &str, source_file: &str) -> Option<String> {
		let python_files = self.file_registry.get("python")?;
		let source_path = Path::new(source_file);
		let source_dir = source_path.parent()?;

		if import_path.starts_with('.') {
			// Relative import: .module or ..module
			let dots = import_path.chars().take_while(|&c| c == '.').count();
			let module_name = &import_path[dots..];

			let mut target_dir = source_dir;
			for _ in 1..dots {
				target_dir = target_dir.parent()?;
			}

			self.find_python_module(module_name, target_dir, python_files)
		} else {
			// Absolute import - look from project root or in same package
			self.find_python_module(import_path, source_dir, python_files)
				.or_else(|| {
					// Try from project root
					let project_root = self.find_project_root(source_file)?;
					self.find_python_module(import_path, Path::new(&project_root), python_files)
				})
		}
	}

	/// Resolve Go package imports to directories
	fn resolve_go_import(&self, import_path: &str, source_file: &str) -> Option<String> {
		let go_files = self.file_registry.get("go")?;
		let source_path = Path::new(source_file);
		let source_dir = source_path.parent()?;

		if import_path.starts_with("./") || import_path.starts_with("../") {
			// Relative import
			let relative_path = Path::new(import_path);
			let resolved_path = source_dir.join(relative_path);
			self.find_go_package(&resolved_path, go_files)
		} else {
			// Absolute import - look for package directory
			self.find_go_package_by_name(import_path, go_files)
		}
	}

	/// Resolve PHP use statements to file paths
	fn resolve_php_import(&self, import_path: &str, source_file: &str) -> Option<String> {
		let php_files = self.file_registry.get("php")?;
		let source_path = Path::new(source_file);
		let source_dir = source_path.parent()?;

		// Convert namespace to file path
		let file_path = import_path.replace("\\", "/");

		// Try different common PHP patterns
		let candidates = vec![
			format!("{}.php", file_path),
			format!("{}/index.php", file_path),
			format!("src/{}.php", file_path),
			format!("lib/{}.php", file_path),
		];

		for candidate in &candidates {
			if let Some(found) = self.find_file_in_list(candidate, php_files) {
				return Some(found);
			}
		}

		// Try relative to source directory
		for candidate in &candidates {
			let relative_path = source_dir.join(candidate);
			let relative_path_str = relative_path.to_string_lossy().to_string();
			if let Some(found) = self.find_file_in_list(&relative_path_str, php_files) {
				return Some(found);
			}
		}

		None
	}

	/// Resolve C++ #include statements to file paths
	fn resolve_cpp_import(&self, import_path: &str, source_file: &str) -> Option<String> {
		let cpp_files = self
			.file_registry
			.get("cpp")
			.or_else(|| self.file_registry.get("c"))?;
		let source_path = Path::new(source_file);
		let source_dir = source_path.parent()?;

		if import_path.starts_with('"') && import_path.ends_with('"') {
			// Local include: #include "header.h"
			let header_name = &import_path[1..import_path.len() - 1];
			let header_path = source_dir.join(header_name);
			self.find_file_in_list(&header_path.to_string_lossy(), cpp_files)
		} else if import_path.starts_with('<') && import_path.ends_with('>') {
			// System include: #include <iostream>
			// For now, skip system includes as they're not in our codebase
			None
		} else {
			// Direct path
			self.find_file_in_list(import_path, cpp_files)
		}
	}

	/// Resolve Ruby require statements to file paths
	fn resolve_ruby_import(&self, import_path: &str, source_file: &str) -> Option<String> {
		let ruby_files = self.file_registry.get("ruby")?;
		let source_path = Path::new(source_file);
		let source_dir = source_path.parent()?;

		if import_path.starts_with("./") || import_path.starts_with("../") {
			// Relative require
			let relative_path = Path::new(import_path);
			let resolved_path = source_dir.join(relative_path);
			self.find_ruby_file_with_extensions(&resolved_path, ruby_files)
		} else {
			// Absolute require - look for gem or local file
			self.find_ruby_file_with_extensions(Path::new(import_path), ruby_files)
		}
	}

	/// Resolve Bash source statements to file paths
	fn resolve_bash_import(&self, import_path: &str, source_file: &str) -> Option<String> {
		let bash_files = self.file_registry.get("bash")?;
		let source_path = Path::new(source_file);
		let source_dir = source_path.parent()?;

		if import_path.starts_with("./") || import_path.starts_with("../") {
			// Relative source
			let relative_path = Path::new(import_path);
			let resolved_path = source_dir.join(relative_path);
			self.find_file_in_list(&resolved_path.to_string_lossy(), bash_files)
		} else {
			// Absolute source
			self.find_file_in_list(import_path, bash_files)
		}
	}

	// Helper methods for language-specific resolution

	fn resolve_rust_module_path(
		&self,
		module_path: &str,
		source_file: &str,
		rust_files: &[String],
	) -> Option<String> {
		let parts: Vec<&str> = module_path.split("::").collect();
		if parts.is_empty() {
			return None;
		}

		// Find crate root
		let crate_root = self.find_rust_crate_root(source_file, rust_files)?;
		let crate_dir = Path::new(&crate_root).parent()?;

		// Build path from module parts
		let mut current_path = crate_dir.to_path_buf();
		for (i, part) in parts.iter().enumerate() {
			if i == parts.len() - 1 {
				// Last part could be a file or module
				let file_path = current_path.join(format!("{}.rs", part));
				let file_path_str = file_path.to_string_lossy();
				if let Some(found) = self.find_file_in_list(&file_path_str, rust_files) {
					return Some(found);
				}

				let mod_path = current_path.join(part).join("mod.rs");
				let mod_path_str = mod_path.to_string_lossy();
				if let Some(found) = self.find_file_in_list(&mod_path_str, rust_files) {
					return Some(found);
				}
			} else {
				current_path = current_path.join(part);
			}
		}

		None
	}

	fn resolve_rust_relative_path(
		&self,
		module_path: &str,
		base_dir: &Path,
		rust_files: &[String],
	) -> Option<String> {
		let parts: Vec<&str> = module_path.split("::").collect();
		if parts.is_empty() {
			return None;
		}

		let mut current_path = base_dir.to_path_buf();
		for (i, part) in parts.iter().enumerate() {
			if i == parts.len() - 1 {
				// Last part - look for file
				let file_path = current_path.join(format!("{}.rs", part));
				if let Some(found) =
					self.find_file_in_list(&file_path.to_string_lossy(), rust_files)
				{
					return Some(found);
				}
			} else {
				current_path = current_path.join(part);
			}
		}

		None
	}

	fn find_rust_file_in_dir(
		&self,
		filename: &str,
		dir: &Path,
		rust_files: &[String],
	) -> Option<String> {
		let file_path = dir.join(filename);
		self.find_file_in_list(&file_path.to_string_lossy(), rust_files)
	}

	fn find_rust_crate_root(&self, source_file: &str, rust_files: &[String]) -> Option<String> {
		let source_path = Path::new(source_file);
		let mut current_dir = source_path.parent()?;

		loop {
			// Look for lib.rs or main.rs
			let lib_path = current_dir.join("lib.rs");
			if let Some(found) = self.find_file_in_list(&lib_path.to_string_lossy(), rust_files) {
				return Some(found);
			}

			let main_path = current_dir.join("main.rs");
			if let Some(found) = self.find_file_in_list(&main_path.to_string_lossy(), rust_files) {
				return Some(found);
			}

			// Move up one directory
			current_dir = current_dir.parent()?;
		}
	}

	fn find_js_file_with_extensions(
		&self,
		base_path: &Path,
		js_files: &[String],
	) -> Option<String> {
		let extensions = ["", ".js", ".ts", ".jsx", ".tsx", "/index.js", "/index.ts"];

		for ext in &extensions {
			let file_path = if ext.is_empty() {
				base_path.to_path_buf()
			} else {
				PathBuf::from(format!("{}{}", base_path.to_string_lossy(), ext))
			};

			if let Some(found) = self.find_file_in_list(&file_path.to_string_lossy(), js_files) {
				return Some(found);
			}
		}

		None
	}

	fn find_python_module(
		&self,
		module_name: &str,
		base_dir: &Path,
		python_files: &[String],
	) -> Option<String> {
		let module_parts: Vec<&str> = module_name.split('.').collect();
		let mut current_path = base_dir.to_path_buf();

		for (i, part) in module_parts.iter().enumerate() {
			if i == module_parts.len() - 1 {
				// Last part - look for .py file or __init__.py in directory
				let file_path = current_path.join(format!("{}.py", part));
				if let Some(found) =
					self.find_file_in_list(&file_path.to_string_lossy(), python_files)
				{
					return Some(found);
				}

				let init_path = current_path.join(part).join("__init__.py");
				if let Some(found) =
					self.find_file_in_list(&init_path.to_string_lossy(), python_files)
				{
					return Some(found);
				}
			} else {
				current_path = current_path.join(part);
			}
		}

		None
	}

	fn find_go_package(&self, package_dir: &Path, go_files: &[String]) -> Option<String> {
		// Look for any .go file in the package directory
		for go_file in go_files {
			let file_path = Path::new(go_file);
			if let Some(file_dir) = file_path.parent() {
				if file_dir == package_dir {
					return Some(go_file.clone());
				}
			}
		}

		None
	}

	fn find_go_package_by_name(&self, package_name: &str, go_files: &[String]) -> Option<String> {
		// Look for package directory by name
		for go_file in go_files {
			let file_path = Path::new(go_file);
			if let Some(file_dir) = file_path.parent() {
				if let Some(dir_name) = file_dir.file_name() {
					if dir_name.to_string_lossy() == package_name {
						return Some(go_file.clone());
					}
				}
			}
		}

		None
	}

	fn find_ruby_file_with_extensions(
		&self,
		base_path: &Path,
		ruby_files: &[String],
	) -> Option<String> {
		let extensions = ["", ".rb"];

		for ext in &extensions {
			let file_path = if ext.is_empty() {
				base_path.to_path_buf()
			} else {
				PathBuf::from(format!("{}{}", base_path.to_string_lossy(), ext))
			};

			if let Some(found) = self.find_file_in_list(&file_path.to_string_lossy(), ruby_files) {
				return Some(found);
			}
		}

		None
	}

	fn find_project_root(&self, source_file: &str) -> Option<String> {
		let source_path = Path::new(source_file);
		let mut current_dir = source_path.parent()?;

		loop {
			// Look for common project root indicators
			let indicators = [
				"Cargo.toml",
				"package.json",
				"setup.py",
				"go.mod",
				"composer.json",
			];
			for indicator in &indicators {
				let indicator_path = current_dir.join(indicator);
				if indicator_path.exists() {
					return Some(current_dir.to_string_lossy().to_string());
				}
			}

			// Move up one directory
			if let Some(parent) = current_dir.parent() {
				current_dir = parent;
			} else {
				break;
			}
		}

		None
	}

	fn find_file_in_list(&self, target_path: &str, file_list: &[String]) -> Option<String> {
		let target = Path::new(target_path);

		for file_path in file_list {
			let file = Path::new(file_path);

			// Exact match
			if file == target {
				return Some(file_path.clone());
			}

			// Match by canonical path if possible
			if let (Ok(file_canonical), Ok(target_canonical)) =
				(file.canonicalize(), target.canonicalize())
			{
				if file_canonical == target_canonical {
					return Some(file_path.clone());
				}
			}

			// Match by file name if paths are similar
			if let (Some(file_name), Some(target_name)) = (file.file_name(), target.file_name()) {
				if file_name == target_name {
					// Additional check: ensure directories are related
					if let (Some(file_dir), Some(target_dir)) = (file.parent(), target.parent()) {
						let file_dir_str = file_dir.to_string_lossy().to_string();
						let target_dir_str = target_dir.to_string_lossy().to_string();
						if file_dir_str.contains(&target_dir_str)
							|| target_dir_str.contains(&file_dir_str)
						{
							return Some(file_path.clone());
						}
					}
				}
			}
		}

		None
	}
}

/// Detect language from file path
fn detect_language_from_path(file_path: &str) -> Option<String> {
	let path = Path::new(file_path);
	let extension = path.extension()?.to_str()?;

	match extension {
		"rs" => Some("rust".to_string()),
		"js" | "mjs" => Some("javascript".to_string()),
		"ts" | "tsx" => Some("typescript".to_string()),
		"py" => Some("python".to_string()),
		"go" => Some("go".to_string()),
		"php" => Some("php".to_string()),
		"cpp" | "cc" | "cxx" | "c++" => Some("cpp".to_string()),
		"c" | "h" => Some("c".to_string()),
		"rb" => Some("ruby".to_string()),
		"sh" | "bash" => Some("bash".to_string()),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_rust_import_resolution() {
		let files = vec![
			"src/lib.rs".to_string(),
			"src/main.rs".to_string(),
			"src/utils.rs".to_string(),
			"src/models/mod.rs".to_string(),
			"src/models/user.rs".to_string(),
		];

		let mut resolver = ImportResolver::new(&files);

		// Test crate import
		let result = resolver.resolve_import("crate::utils", "src/main.rs", "rust");
		assert_eq!(result, Some("src/utils.rs".to_string()));

		// Test module import
		let result = resolver.resolve_import("crate::models::user", "src/main.rs", "rust");
		assert_eq!(result, Some("src/models/user.rs".to_string()));
	}

	#[test]
	fn test_javascript_import_resolution() {
		let files = vec![
			"src/index.js".to_string(),
			"src/utils.js".to_string(),
			"src/components/Button.js".to_string(),
		];

		let mut resolver = ImportResolver::new(&files);

		// Test relative import
		let result = resolver.resolve_import("./utils", "src/index.js", "javascript");
		assert_eq!(result, Some("src/utils.js".to_string()));

		// Test relative import with extension
		let result = resolver.resolve_import("./utils.js", "src/index.js", "javascript");
		assert_eq!(result, Some("src/utils.js".to_string()));
	}

	#[test]
	fn test_python_import_resolution() {
		let files = vec![
			"src/__init__.py".to_string(),
			"src/main.py".to_string(),
			"src/utils.py".to_string(),
			"src/models/__init__.py".to_string(),
			"src/models/user.py".to_string(),
		];

		let mut resolver = ImportResolver::new(&files);

		// Test relative import
		let result = resolver.resolve_import(".utils", "src/main.py", "python");
		assert_eq!(result, Some("src/utils.py".to_string()));

		// Test module import
		let result = resolver.resolve_import("models.user", "src/main.py", "python");
		assert_eq!(result, Some("src/models/user.py".to_string()));
	}
}
