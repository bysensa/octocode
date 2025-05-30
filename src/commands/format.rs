// Octocode - Intelligent Code Indexer and Graph Builder
// Copyright (c) 2025 Muvon Un Limited
// Licensed under the MIT License

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{anyhow, Context, Result};
use clap::Args;

#[derive(Args)]
pub struct FormatArgs {
	/// Show what would be changed without applying (dry-run mode)
	#[arg(long)]
	pub dry_run: bool,

	/// Commit changes after formatting
	#[arg(short, long)]
	pub commit: bool,

	/// Specific files to format (default: all git-tracked and unstaged files)
	pub files: Vec<PathBuf>,

	/// Show verbose output
	#[arg(short, long)]
	pub verbose: bool,
}

pub async fn execute(format_args: &FormatArgs) -> Result<()> {
	let git_root = find_git_root()
		.context("Failed to find git repository root. Make sure you're in a git repository.")?;

	let editorconfig_path = git_root.join(".editorconfig");

	if !editorconfig_path.exists() {
		return Err(anyhow!(".editorconfig file not found in git root: {}", git_root.display()));
	}

	if format_args.verbose {
		println!("Found .editorconfig at: {}", editorconfig_path.display());
		println!("Git root: {}", git_root.display());
	}

	let files_to_format = if format_args.files.is_empty() {
		get_git_files(&git_root)?
	} else {
		// Convert relative paths to absolute and validate they exist
		format_args.files.iter()
			.map(|f| {
				if f.is_absolute() {
					f.clone()
				} else {
					git_root.join(f)
				}
			})
			.filter(|f| f.exists())
			.collect()
	};

	if files_to_format.is_empty() {
		println!("No files to format found.");
		return Ok(());
	}

	if format_args.verbose {
		println!("Found {} files to process", files_to_format.len());
	}

	let mut formatted_files = Vec::new();
	let mut total_changes = 0;

	for file_path in &files_to_format {
		if format_args.verbose {
			println!("Processing: {}", file_path.display());
		}

		let changes = format_file(file_path, !format_args.dry_run, format_args.verbose)
			.with_context(|| format!("Failed to format file: {}", file_path.display()))?;

		if changes > 0 {
			formatted_files.push(file_path.clone());
			total_changes += changes;
		}
	}

	if total_changes == 0 {
		println!("No formatting changes needed.");
		return Ok(());
	}

	let action = if format_args.dry_run { "would be applied" } else { "applied" };
	println!(
		"Formatting complete: {} changes across {} files ({})",
		total_changes,
		formatted_files.len(),
		action
	);

	if !format_args.dry_run && format_args.commit {
		commit_changes(&formatted_files)?;
	}

	Ok(())
}

fn find_git_root() -> Result<PathBuf> {
	let current_dir = std::env::current_dir()
		.context("Failed to get current directory")?;

	let mut path = current_dir.as_path();

	loop {
		if path.join(".git").exists() {
			return Ok(path.to_path_buf());
		}

		match path.parent() {
			Some(parent) => path = parent,
			None => return Err(anyhow!("Not in a git repository")),
		}
	}
}

fn get_git_files(git_root: &Path) -> Result<Vec<PathBuf>> {
	let mut files = Vec::new();

	// Get all tracked files
	let output = Command::new("git")
		.args(["ls-files"])
		.current_dir(git_root)
		.output()
		.context("Failed to execute 'git ls-files'")?;

	if !output.status.success() {
		return Err(anyhow!(
			"Git ls-files failed: {}",
			String::from_utf8_lossy(&output.stderr)
		));
	}

	let tracked_files = String::from_utf8_lossy(&output.stdout);
	for line in tracked_files.lines() {
		if !line.trim().is_empty() {
			files.push(git_root.join(line.trim()));
		}
	}

	// Get untracked files that are not ignored
	let output = Command::new("git")
		.args(["status", "--porcelain", "--untracked-files=all"])
		.current_dir(git_root)
		.output()
		.context("Failed to execute 'git status'")?;

	if !output.status.success() {
		return Err(anyhow!(
			"Git status failed: {}",
			String::from_utf8_lossy(&output.stderr)
		));
	}

	let status_output = String::from_utf8_lossy(&output.stdout);
	for line in status_output.lines() {
		if line.len() >= 3 {
			let status = &line[0..2];
			let file_path = &line[3..];

			// Include untracked files (status starts with ??)
			if status == "??" {
				let full_path = git_root.join(file_path.trim());
				if full_path.exists() && full_path.is_file() {
					files.push(full_path);
				}
			}
		}
	}

	// Filter out files that should be ignored (check if git would ignore them)
	let mut final_files = Vec::new();
	for file in files {
		if is_text_file(&file)? {
			final_files.push(file);
		}
	}

	Ok(final_files)
}

fn is_text_file(file_path: &Path) -> Result<bool> {
	// Check if git considers this file as text
	let output = Command::new("git")
		.args(["check-attr", "--all", &file_path.to_string_lossy()])
		.output()
		.context("Failed to execute 'git check-attr'")?;

	if !output.status.success() {
		// If git check-attr fails, fall back to simple heuristics
		return Ok(is_likely_text_file(file_path));
	}

	let attr_output = String::from_utf8_lossy(&output.stdout);

	// Check if file is marked as binary
	if attr_output.contains("binary: set") || attr_output.contains("binary: true") {
		return Ok(false);
	}

	// If no binary attribute, assume it's text if it has a reasonable extension or passes heuristic
	Ok(is_likely_text_file(file_path))
}

fn is_likely_text_file(file_path: &Path) -> bool {
	// Common text file extensions
	let text_extensions = [
		"rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "kt", "scala",
		"cpp", "c", "h", "hpp", "cc", "cxx", "cs", "php", "rb", "pl", "pm",
		"sh", "bash", "zsh", "fish", "ps1", "bat", "cmd",
		"html", "htm", "xml", "xhtml", "svg", "css", "scss", "sass", "less",
		"json", "yaml", "yml", "toml", "ini", "cfg", "conf", "config",
		"md", "markdown", "rst", "txt", "text", "rtf",
		"sql", "ddl", "dml", "graphql", "gql",
		"dockerfile", "makefile", "cmake", "gradle", "maven",
		"vue", "svelte", "astro", "ejs", "hbs", "mustache",
		"r", "m", "swift", "dart", "lua", "nim", "zig", "v",
	];

	if let Some(extension) = file_path.extension() {
		let ext = extension.to_string_lossy().to_lowercase();
		if text_extensions.contains(&ext.as_str()) {
			return true;
		}
	}

	// Check filename patterns
	let filename = file_path.file_name()
		.map(|n| n.to_string_lossy().to_lowercase())
		.unwrap_or_default();

	let text_filenames = [
		"dockerfile", "makefile", "rakefile", "gemfile", "podfile",
		"license", "readme", "changelog", "authors", "contributors",
		"copying", "install", "news", "todo", "version",
		".gitignore", ".gitattributes", ".editorconfig", ".eslintrc",
		".prettierrc", ".babelrc", ".nvmrc", ".rustfmt.toml",
	];

	for pattern in &text_filenames {
		if filename.contains(pattern) {
			return true;
		}
	}

	// If no extension and not a known filename, check if file starts with shebang
	if file_path.extension().is_none() {
		if let Ok(content) = fs::read_to_string(file_path) {
			if content.starts_with("#!") {
				return true;
			}
		}
	}

	false
}

fn format_file(
	file_path: &Path,
	apply: bool,
	verbose: bool,
) -> Result<usize> {
	let content = fs::read_to_string(file_path)
		.with_context(|| format!("Failed to read file: {}", file_path.display()))?;

	// Get EditorConfig properties for this file
	let properties = editorconfig::get_config(file_path)
		.map_err(|e| anyhow!("Failed to get editorconfig properties for {}: {}", file_path.display(), e))?;

	let mut new_content = content.clone();
	let mut changes = 0;

	// Apply EditorConfig rules

	// 1. Handle line endings
	if let Some(end_of_line) = properties.get("end_of_line") {
		let target_ending = match end_of_line.as_str() {
			"lf" => "\n",
			"crlf" => "\r\n",
			"cr" => "\r",
			_ => "\n", // default
		};

		// Normalize all line endings to \n first, then apply target
		let normalized = new_content.replace("\r\n", "\n").replace('\r', "\n");
		if target_ending != "\n" {
			new_content = normalized.replace('\n', target_ending);
		} else {
			new_content = normalized;
		}

		if new_content != content {
			changes += 1;
			if verbose {
				println!("  - Fixed line endings");
			}
		}
	}

	// 2. Handle charset (for now, just ensure UTF-8)
	if let Some(charset) = properties.get("charset") {
		if charset == "utf-8" && verbose {
			println!("  - Verified UTF-8 encoding");
		}
	}

	// 3. Handle trailing whitespace
	if let Some(trim_trailing) = properties.get("trim_trailing_whitespace") {
		if trim_trailing == "true" {
			let lines: Vec<&str> = new_content.lines().collect();
			let trimmed_lines: Vec<String> = lines.iter()
				.map(|line| line.trim_end().to_string())
				.collect();

			let line_ending = if new_content.contains("\r\n") {
				"\r\n"
			} else if new_content.contains('\r') {
				"\r"
			} else {
				"\n"
			};

			let trimmed_content = trimmed_lines.join(line_ending);

			// Preserve final newline status
			let ends_with_newline = new_content.ends_with('\n') || new_content.ends_with('\r');
			let mut final_content = trimmed_content;
			if ends_with_newline && !final_content.ends_with('\n') && !final_content.ends_with('\r') {
				final_content.push_str(line_ending);
			}

			if final_content != new_content {
				new_content = final_content;
				changes += 1;
				if verbose {
					println!("  - Trimmed trailing whitespace");
				}
			}
		}
	}

	// 4. Handle final newline
	if let Some(insert_final_newline) = properties.get("insert_final_newline") {
		if insert_final_newline == "true" {
			if !new_content.is_empty() && !new_content.ends_with('\n') && !new_content.ends_with('\r') {
				let line_ending = if new_content.contains("\r\n") {
					"\r\n"
				} else if new_content.contains('\r') {
					"\r"
				} else {
					"\n"
				};
				new_content.push_str(line_ending);
				changes += 1;
				if verbose {
					println!("  - Added final newline");
				}
			}
		} else if insert_final_newline == "false" {
			if new_content.ends_with('\n') || new_content.ends_with('\r') {
				new_content = new_content.trim_end_matches(&['\n', '\r'][..]).to_string();
				changes += 1;
				if verbose {
					println!("  - Removed final newline");
				}
			}
		}
	}

	// 5. Handle indentation (basic conversion)
	if let (Some(indent_style), Some(indent_size_str)) = (properties.get("indent_style"), properties.get("indent_size")) {
		if let Ok(indent_size) = indent_size_str.parse::<usize>() {
			let lines: Vec<&str> = new_content.lines().collect();
			let mut converted_lines = Vec::new();
			let mut indent_changes = 0;

			for line in lines {
				if line.trim().is_empty() {
					converted_lines.push(line.to_string());
					continue;
				}

				let (leading_whitespace, rest) = split_leading_whitespace(line);
				let converted_indent = convert_indentation(
					&leading_whitespace,
					indent_style,
					indent_size,
				);

				if converted_indent != leading_whitespace {
					indent_changes += 1;
				}

				converted_lines.push(format!("{}{}", converted_indent, rest));
			}

			if indent_changes > 0 {
				let line_ending = if new_content.contains("\r\n") {
					"\r\n"
				} else if new_content.contains('\r') {
					"\r"
				} else {
					"\n"
				};

				new_content = converted_lines.join(line_ending);

				// Preserve final newline status
				if content.ends_with('\n') || content.ends_with('\r') {
					if !new_content.ends_with('\n') && !new_content.ends_with('\r') {
						new_content.push_str(line_ending);
					}
				}

				changes += 1;
				if verbose {
					println!("  - Converted indentation ({} lines)", indent_changes);
				}
			}
		}
	}

	// Apply changes if requested
	if apply && changes > 0 {
		fs::write(file_path, &new_content)
			.with_context(|| format!("Failed to write file: {}", file_path.display()))?;
	}

	Ok(changes)
}

fn split_leading_whitespace(line: &str) -> (String, &str) {
	let trimmed = line.trim_start();
	let leading_len = line.len() - trimmed.len();
	(line[..leading_len].to_string(), trimmed)
}

fn convert_indentation(
	whitespace: &str,
	target_style: &str,
	target_size: usize,
) -> String {
	// Convert existing indentation to logical level
	let logical_level = match target_style {
		"tab" => {
			// When converting to tabs, calculate spaces equivalent
			whitespace.chars().fold(0, |acc, c| {
				match c {
					'\t' => acc + target_size,
					' ' => acc + 1,
					_ => acc,
				}
			}) / target_size
		}
		"space" => {
			// When converting to spaces, calculate tab equivalent
			whitespace.chars().fold(0, |acc, c| {
				match c {
					'\t' => acc + target_size,
					' ' => acc + 1,
					_ => acc,
				}
			}) / target_size
		}
		_ => return whitespace.to_string(), // Unknown style, keep as is
	};

	// Generate new indentation
	match target_style {
		"tab" => "\t".repeat(logical_level),
		"space" => " ".repeat(logical_level * target_size),
		_ => whitespace.to_string(), // Unknown style, keep as is
	}
}

fn commit_changes(files: &[PathBuf]) -> Result<()> {
	// Add files to git
	for file in files {
		let output = Command::new("git")
			.args(["add", &file.to_string_lossy()])
			.output()
			.context("Failed to execute git add command")?;

		if !output.status.success() {
			return Err(anyhow!(
				"Git add failed for {}: {}",
				file.display(),
				String::from_utf8_lossy(&output.stderr)
			));
		}
	}

	// Create commit message
	let commit_message = "Format code according to .editorconfig";

	// Commit changes
	let output = Command::new("git")
		.args(["commit", "-m", commit_message])
		.output()
		.context("Failed to execute git commit command")?;

	if !output.status.success() {
		return Err(anyhow!(
			"Git commit failed: {}",
			String::from_utf8_lossy(&output.stderr)
		));
	}

	println!("Successfully committed formatting changes:");
	println!("  Message: {}", commit_message);
	println!("  Files: {}", files.len());

	Ok(())
}
