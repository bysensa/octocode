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

use anyhow::{Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use octocode::config::Config;

#[derive(Args, Debug)]
pub struct ReleaseArgs {
	/// Changelog file path (default: CHANGELOG.md)
	#[arg(short, long, default_value = "CHANGELOG.md")]
	pub changelog: String,

	/// Skip confirmation prompt
	#[arg(short, long)]
	pub yes: bool,

	/// Dry run - show what would be done without making changes
	#[arg(short, long)]
	pub dry_run: bool,

	/// Force a specific version instead of AI calculation
	#[arg(short, long)]
	pub force_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitAnalysis {
	pub commits: Vec<CommitInfo>,
	pub breaking_changes: Vec<String>,
	pub features: Vec<String>,
	pub fixes: Vec<String>,
	pub other_changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
	pub hash: String,
	pub message: String,
	pub author: String,
	pub date: String,
	pub commit_type: String,
	pub scope: Option<String>,
	pub description: String,
	pub breaking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionCalculation {
	pub current_version: String,
	pub new_version: String,
	pub version_type: String, // major, minor, patch
	pub reasoning: String,
}

#[derive(Debug, Clone)]
pub enum ProjectType {
	Rust(PathBuf), // Cargo.toml
	Node(PathBuf), // package.json
	Php(PathBuf),  // composer.json
	Go(PathBuf),   // go.mod
	Unknown,
}

pub async fn execute(config: &Config, args: &ReleaseArgs) -> Result<()> {
	let current_dir = std::env::current_dir()?;

	// Check if we're in a git repository
	if !current_dir.join(".git").exists() {
		return Err(anyhow::anyhow!("❌ Not in a git repository!"));
	}

	println!("🚀 Starting release process...\n");

	// Detect project type
	let project_type = detect_project_type(&current_dir)?;
	println!(
		"📦 Project type detected: {}",
		format_project_type(&project_type)
	);

	// Get current version from project files or git tags
	let current_version = get_current_version(&project_type).await?;
	println!("📌 Current version: {}", current_version);

	// Get latest tag to determine commit range
	let last_tag = get_latest_tag().await?;
	let commit_range = if let Some(ref tag) = last_tag {
		format!("{}..HEAD", tag)
	} else {
		"HEAD".to_string()
	};

	println!(
		"📋 Analyzing commits since: {}",
		last_tag.as_deref().unwrap_or("initial commit")
	);

	// Analyze commits since last tag
	let commit_analysis = analyze_commits(&commit_range).await?;

	if commit_analysis.commits.is_empty() {
		println!("✅ No new commits since last release. Nothing to release.");
		return Ok(());
	}

	println!(
		"📊 Found {} commits to analyze",
		commit_analysis.commits.len()
	);

	// Calculate new version using AI
	let version_calculation = if let Some(forced_version) = &args.force_version {
		VersionCalculation {
			current_version: current_version.clone(),
			new_version: forced_version.clone(),
			version_type: "forced".to_string(),
			reasoning: "Version forced by user".to_string(),
		}
	} else {
		calculate_version_with_ai(config, &current_version, &commit_analysis).await?
	};

	println!("\n🎯 Version calculation:");
	println!("   Current: {}", version_calculation.current_version);
	println!("   New:     {}", version_calculation.new_version);
	println!("   Type:    {}", version_calculation.version_type);
	println!("   Reason:  {}", version_calculation.reasoning);

	// Generate changelog content
	let changelog_content =
		generate_changelog_content(&version_calculation, &commit_analysis).await?;

	println!("\n📝 Generated changelog entry:");
	println!("═══════════════════════════════════");
	println!("{}", changelog_content);
	println!("═══════════════════════════════════");

	if args.dry_run {
		println!("\n🔍 DRY RUN - No changes would be made");
		return Ok(());
	}

	// Confirm with user (unless --yes flag is used)
	if !args.yes {
		print!(
			"\nProceed with release {}? [y/N] ",
			version_calculation.new_version
		);
		io::stdout().flush()?;

		let mut input = String::new();
		io::stdin().read_line(&mut input)?;

		if !input.trim().to_lowercase().starts_with('y') {
			println!("❌ Release cancelled.");
			return Ok(());
		}
	}

	println!("\n🔄 Creating release...");

	// Update project files with new version
	update_project_version(&project_type, &version_calculation.new_version).await?;
	println!("✅ Updated project files");

	// Update changelog
	update_changelog(&args.changelog, &changelog_content).await?;
	println!("✅ Updated {}", args.changelog);

	// Stage changes
	stage_release_files(&args.changelog, &project_type).await?;
	println!("✅ Staged release files");

	// Create release commit
	let commit_message = format!("chore(release): {}", version_calculation.new_version);
	create_commit(&commit_message).await?;
	println!("✅ Created release commit");

	// Create git tag
	create_tag(&version_calculation.new_version, &changelog_content).await?;
	println!("✅ Created git tag: {}", version_calculation.new_version);

	println!(
		"\n🎉 Release {} created successfully!",
		version_calculation.new_version
	);
	println!("💡 Don't forget to push with: git push origin main --tags");

	Ok(())
}

fn detect_project_type(dir: &Path) -> Result<ProjectType> {
	if dir.join("Cargo.toml").exists() {
		Ok(ProjectType::Rust(dir.join("Cargo.toml")))
	} else if dir.join("package.json").exists() {
		Ok(ProjectType::Node(dir.join("package.json")))
	} else if dir.join("composer.json").exists() {
		Ok(ProjectType::Php(dir.join("composer.json")))
	} else if dir.join("go.mod").exists() {
		Ok(ProjectType::Go(dir.join("go.mod")))
	} else {
		Ok(ProjectType::Unknown)
	}
}

fn format_project_type(project_type: &ProjectType) -> String {
	match project_type {
		ProjectType::Rust(_) => "Rust (Cargo.toml)".to_string(),
		ProjectType::Node(_) => "Node.js (package.json)".to_string(),
		ProjectType::Php(_) => "PHP (composer.json)".to_string(),
		ProjectType::Go(_) => "Go (go.mod)".to_string(),
		ProjectType::Unknown => "Unknown (no project file detected)".to_string(),
	}
}

async fn get_current_version(project_type: &ProjectType) -> Result<String> {
	match project_type {
		ProjectType::Rust(cargo_path) => {
			let content = fs::read_to_string(cargo_path)?;
			if let Some(version_line) = content
				.lines()
				.find(|line| line.trim_start().starts_with("version"))
			{
				if let Some(version) = extract_version_from_line(version_line) {
					return Ok(version);
				}
			}
		}
		ProjectType::Node(package_path) => {
			let content = fs::read_to_string(package_path)?;
			let package: serde_json::Value = serde_json::from_str(&content)?;
			if let Some(version) = package.get("version").and_then(|v| v.as_str()) {
				return Ok(version.to_string());
			}
		}
		ProjectType::Php(composer_path) => {
			let content = fs::read_to_string(composer_path)?;
			let composer: serde_json::Value = serde_json::from_str(&content)?;
			if let Some(version) = composer.get("version").and_then(|v| v.as_str()) {
				return Ok(version.to_string());
			}
		}
		ProjectType::Go(go_mod_path) => {
			// Check for VERSION file in Go projects
			let version_file = go_mod_path.parent().unwrap().join("VERSION");
			if version_file.exists() {
				let content = fs::read_to_string(version_file)?;
				return Ok(content.trim().to_string());
			}
			// Fall back to git tags if no VERSION file
		}
		ProjectType::Unknown => {}
	}

	// Fallback to git tags
	if let Ok(Some(tag)) = get_latest_tag().await {
		// Remove 'v' prefix if present
		let version = tag.strip_prefix('v').unwrap_or(&tag);
		Ok(version.to_string())
	} else {
		Ok("0.1.0".to_string())
	}
}

fn extract_version_from_line(line: &str) -> Option<String> {
	// Extract version from line like: version = "1.0.0"
	if let Some(start) = line.find('"') {
		if let Some(end) = line[start + 1..].find('"') {
			return Some(line[start + 1..start + 1 + end].to_string());
		}
	}
	// Try single quotes: version = '1.0.0'
	if let Some(start) = line.find('\'') {
		if let Some(end) = line[start + 1..].find('\'') {
			return Some(line[start + 1..start + 1 + end].to_string());
		}
	}
	None
}

async fn get_latest_tag() -> Result<Option<String>> {
	let output = Command::new("git")
		.args(["describe", "--tags", "--abbrev=0"])
		.output()?;

	if output.status.success() {
		let tag = String::from_utf8(output.stdout)?;
		Ok(Some(tag.trim().to_string()))
	} else {
		Ok(None)
	}
}

async fn analyze_commits(commit_range: &str) -> Result<CommitAnalysis> {
	let output = Command::new("git")
		.args(["log", "--format=%H|%an|%ad|%s", "--date=iso", commit_range])
		.output()?;

	if !output.status.success() {
		return Err(anyhow::anyhow!(
			"Failed to get commit log: {}",
			String::from_utf8_lossy(&output.stderr)
		));
	}

	let log_output = String::from_utf8(output.stdout)?;
	let mut commits = Vec::new();
	let mut breaking_changes = Vec::new();
	let mut features = Vec::new();
	let mut fixes = Vec::new();
	let mut other_changes = Vec::new();

	for line in log_output.lines() {
		if line.trim().is_empty() {
			continue;
		}

		let parts: Vec<&str> = line.split('|').collect();
		if parts.len() < 4 {
			continue;
		}

		let hash = parts[0].to_string();
		let author = parts[1].to_string();
		let date = parts[2].to_string();
		let message = parts[3].to_string();

		let (commit_type, scope, description, breaking) = parse_conventional_commit(&message);

		let commit_info = CommitInfo {
			hash: hash.clone(),
			message: message.clone(),
			author,
			date,
			commit_type: commit_type.clone(),
			scope,
			description: description.clone(),
			breaking,
		};

		commits.push(commit_info);

		// Categorize changes
		if breaking {
			breaking_changes.push(format!("**{}**: {}", commit_type, description));
		} else {
			match commit_type.as_str() {
				"feat" => features.push(description),
				"fix" => fixes.push(description),
				_ => other_changes.push(format!("{}: {}", commit_type, description)),
			}
		}
	}

	Ok(CommitAnalysis {
		commits,
		breaking_changes,
		features,
		fixes,
		other_changes,
	})
}

fn parse_conventional_commit(message: &str) -> (String, Option<String>, String, bool) {
	let breaking = message.contains("BREAKING CHANGE") || message.contains('!');

	// Try to parse conventional commit format: type(scope): description
	if let Some(colon_pos) = message.find(':') {
		let prefix = &message[..colon_pos];
		let description = message[colon_pos + 1..].trim().to_string();

		if let Some(paren_start) = prefix.find('(') {
			if let Some(paren_end) = prefix.find(')') {
				let commit_type = prefix[..paren_start].trim().replace('!', "");
				let scope = Some(prefix[paren_start + 1..paren_end].to_string());
				return (commit_type, scope, description, breaking);
			}
		}

		let commit_type = prefix.trim().replace('!', "");
		return (commit_type, None, description, breaking);
	}

	// Fallback: try to detect type from message start
	let lower_message = message.to_lowercase();
	let commit_type = if lower_message.starts_with("feat") {
		"feat"
	} else if lower_message.starts_with("fix") {
		"fix"
	} else if lower_message.starts_with("docs") {
		"docs"
	} else if lower_message.starts_with("style") {
		"style"
	} else if lower_message.starts_with("refactor") {
		"refactor"
	} else if lower_message.starts_with("test") {
		"test"
	} else {
		"chore"
	};

	(commit_type.to_string(), None, message.to_string(), breaking)
}

async fn calculate_version_with_ai(
	config: &Config,
	current_version: &str,
	analysis: &CommitAnalysis,
) -> Result<VersionCalculation> {
	let analysis_json = serde_json::to_string_pretty(analysis)?;

	let prompt = format!(
		"Analyze the following git commits and calculate the next semantic version.\n\n\
        CURRENT VERSION: {}\n\n\
        COMMIT ANALYSIS:\n{}\n\n\
        RULES:\n\
        - MAJOR (x.0.0): Breaking changes or BREAKING CHANGE in commits\n\
        - MINOR (0.x.0): New features (feat) without breaking changes\n\
        - PATCH (0.0.x): Bug fixes (fix) and other changes without new features\n\
        - Follow semantic versioning strictly\n\
        - Consider the impact and scope of changes\n\n\
        Respond with JSON in this exact format:\n\
        {{\n\
        \"current_version\": \"{}\",\n\
        \"new_version\": \"X.Y.Z\",\n\
        \"version_type\": \"major|minor|patch\",\n\
        \"reasoning\": \"Brief explanation of version choice\"\n\
        }}",
		current_version, analysis_json, current_version
	);

	match call_llm_for_version_calculation(&prompt, config).await {
		Ok(response) => {
			// Try to parse JSON response
			if let Ok(calculation) = serde_json::from_str::<VersionCalculation>(&response) {
				Ok(calculation)
			} else {
				// Fallback to manual calculation
				calculate_version_fallback(current_version, analysis)
			}
		}
		Err(e) => {
			eprintln!(
				"Warning: LLM call failed ({}), using fallback calculation",
				e
			);
			calculate_version_fallback(current_version, analysis)
		}
	}
}

fn calculate_version_fallback(
	current_version: &str,
	analysis: &CommitAnalysis,
) -> Result<VersionCalculation> {
	let parts: Vec<&str> = current_version.split('.').collect();
	if parts.len() != 3 {
		return Err(anyhow::anyhow!(
			"Invalid version format: {}",
			current_version
		));
	}

	let major: u32 = parts[0].parse().context("Invalid major version")?;
	let minor: u32 = parts[1].parse().context("Invalid minor version")?;
	let patch: u32 = parts[2].parse().context("Invalid patch version")?;

	let (new_version, version_type, reasoning) = if !analysis.breaking_changes.is_empty() {
		(
			format!("{}.0.0", major + 1),
			"major",
			"Breaking changes detected",
		)
	} else if !analysis.features.is_empty() {
		(
			format!("{}.{}.0", major, minor + 1),
			"minor",
			"New features added",
		)
	} else if !analysis.fixes.is_empty() || !analysis.other_changes.is_empty() {
		(
			format!("{}.{}.{}", major, minor, patch + 1),
			"patch",
			"Bug fixes and improvements",
		)
	} else {
		(
			format!("{}.{}.{}", major, minor, patch + 1),
			"patch",
			"Miscellaneous changes",
		)
	};

	Ok(VersionCalculation {
		current_version: current_version.to_string(),
		new_version,
		version_type: version_type.to_string(),
		reasoning: reasoning.to_string(),
	})
}

async fn call_llm_for_version_calculation(prompt: &str, config: &Config) -> Result<String> {
	use reqwest::Client;
	use serde_json::{json, Value};

	let client = Client::new();

	// Get API key
	let api_key = if let Some(key) = &config.openrouter.api_key {
		key.clone()
	} else if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
		key
	} else {
		return Err(anyhow::anyhow!("No OpenRouter API key found"));
	};

	// Prepare the request
	let payload = json!({
		"model": config.openrouter.model,
		"messages": [
			{
				"role": "user",
				"content": prompt
			}
		],
		"temperature": 0.1,
		"max_tokens": 300
	});

	let response = client
		.post(format!(
			"{}/chat/completions",
			config.openrouter.base_url.trim_end_matches('/')
		))
		.header("Authorization", format!("Bearer {}", api_key))
		.header("HTTP-Referer", "https://github.com/muvon/octocode")
		.header("X-Title", "Octocode")
		.header("Content-Type", "application/json")
		.json(&payload)
		.timeout(std::time::Duration::from_secs(config.openrouter.timeout))
		.send()
		.await?;

	if !response.status().is_success() {
		let error_text = response.text().await?;
		return Err(anyhow::anyhow!("LLM API error: {}", error_text));
	}

	let response_json: Value = response.json().await?;

	let message = response_json
		.get("choices")
		.and_then(|choices| choices.get(0))
		.and_then(|choice| choice.get("message"))
		.and_then(|message| message.get("content"))
		.and_then(|content| content.as_str())
		.ok_or_else(|| anyhow::anyhow!("Invalid response format from LLM"))?;

	Ok(message.to_string())
}

async fn generate_changelog_content(
	version: &VersionCalculation,
	analysis: &CommitAnalysis,
) -> Result<String> {
	let mut content = String::new();
	let date = chrono::Utc::now().format("%Y-%m-%d").to_string();

	content.push_str(&format!("## [{}] - {}\n\n", version.new_version, date));

	if !analysis.breaking_changes.is_empty() {
		content.push_str("### ⚠️ BREAKING CHANGES\n\n");
		for change in &analysis.breaking_changes {
			content.push_str(&format!("- {}\n", change));
		}
		content.push('\n');
	}

	if !analysis.features.is_empty() {
		content.push_str("### ✨ Features\n\n");
		for feature in &analysis.features {
			content.push_str(&format!("- {}\n", feature));
		}
		content.push('\n');
	}

	if !analysis.fixes.is_empty() {
		content.push_str("### 🐛 Bug Fixes\n\n");
		for fix in &analysis.fixes {
			content.push_str(&format!("- {}\n", fix));
		}
		content.push('\n');
	}

	if !analysis.other_changes.is_empty() {
		content.push_str("### 🔧 Other Changes\n\n");
		for change in &analysis.other_changes {
			content.push_str(&format!("- {}\n", change));
		}
		content.push('\n');
	}

	Ok(content)
}

async fn update_project_version(project_type: &ProjectType, new_version: &str) -> Result<()> {
	match project_type {
		ProjectType::Rust(cargo_path) => {
			let content = fs::read_to_string(cargo_path)?;
			let updated_content = update_cargo_version(&content, new_version)?;
			fs::write(cargo_path, updated_content)?;
		}
		ProjectType::Node(package_path) => {
			let content = fs::read_to_string(package_path)?;
			let mut package: serde_json::Value = serde_json::from_str(&content)?;
			package["version"] = serde_json::Value::String(new_version.to_string());
			let updated_content = serde_json::to_string_pretty(&package)?;
			fs::write(package_path, updated_content)?;
		}
		ProjectType::Php(composer_path) => {
			let content = fs::read_to_string(composer_path)?;
			let mut composer: serde_json::Value = serde_json::from_str(&content)?;
			composer["version"] = serde_json::Value::String(new_version.to_string());
			let updated_content = serde_json::to_string_pretty(&composer)?;
			fs::write(composer_path, updated_content)?;
		}
		ProjectType::Go(go_mod_path) => {
			// For Go projects, create/update a VERSION file in the same directory as go.mod
			let version_file = go_mod_path.parent().unwrap().join("VERSION");
			fs::write(version_file, new_version)?;
		}
		ProjectType::Unknown => {
			// No project file to update
		}
	}
	Ok(())
}

fn update_cargo_version(content: &str, new_version: &str) -> Result<String> {
	let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

	for line in &mut lines {
		if line.trim_start().starts_with("version") && line.contains('=') {
			// Replace the version value while preserving formatting
			if let Some(equals_pos) = line.find('=') {
				let prefix = &line[..equals_pos + 1];
				let suffix_start = line[equals_pos + 1..].trim_start();

				// Determine quote style
				let quote_char = if suffix_start.starts_with('"') {
					'"'
				} else {
					'\''
				};

				*line = format!("{} {}{}{}", prefix, quote_char, new_version, quote_char);
				break;
			}
		}
	}

	Ok(lines.join("\n"))
}

async fn update_changelog(changelog_path: &str, new_content: &str) -> Result<()> {
	let changelog_path = Path::new(changelog_path);

	if changelog_path.exists() {
		// Read existing changelog
		let existing_content = fs::read_to_string(changelog_path)?;

		// Find where to insert new content (after the first heading)
		let lines: Vec<&str> = existing_content.lines().collect();
		let mut insert_index = 0;

		// Skip the main title if it exists
		for (i, line) in lines.iter().enumerate() {
			if line.starts_with("# ") {
				insert_index = i + 1;
				// Skip any blank lines after title
				while insert_index < lines.len() && lines[insert_index].trim().is_empty() {
					insert_index += 1;
				}
				break;
			}
		}

		// Insert new content
		let mut new_lines = Vec::new();
		for (i, line) in lines.iter().enumerate() {
			if i == insert_index {
				new_lines.push(new_content.trim_end());
				new_lines.push("");
			}
			new_lines.push(line);
		}

		let updated_content = new_lines.join("\n");
		fs::write(changelog_path, updated_content)?;
	} else {
		// Create new changelog
		let content = format!(
            "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n{}\n",
            new_content.trim_end()
        );
		fs::write(changelog_path, content)?;
	}

	Ok(())
}

async fn stage_release_files(changelog_path: &str, project_type: &ProjectType) -> Result<()> {
	let mut files_to_stage = vec![changelog_path.to_string()];

	// Add project files
	match project_type {
		ProjectType::Rust(path) => files_to_stage.push(path.to_string_lossy().to_string()),
		ProjectType::Node(path) => files_to_stage.push(path.to_string_lossy().to_string()),
		ProjectType::Php(path) => files_to_stage.push(path.to_string_lossy().to_string()),
		ProjectType::Go(go_mod_path) => {
			// Stage the VERSION file for Go projects
			let version_file = go_mod_path.parent().unwrap().join("VERSION");
			files_to_stage.push(version_file.to_string_lossy().to_string());
		}
		ProjectType::Unknown => {}
	}

	for file in files_to_stage {
		let output = Command::new("git").args(["add", &file]).output()?;

		if !output.status.success() {
			return Err(anyhow::anyhow!(
				"Failed to stage {}: {}",
				file,
				String::from_utf8_lossy(&output.stderr)
			));
		}
	}

	Ok(())
}

async fn create_commit(message: &str) -> Result<()> {
	let output = Command::new("git")
		.args(["commit", "-m", message])
		.output()?;

	if !output.status.success() {
		return Err(anyhow::anyhow!(
			"Failed to create commit: {}",
			String::from_utf8_lossy(&output.stderr)
		));
	}

	Ok(())
}

async fn create_tag(version: &str, changelog_content: &str) -> Result<()> {
	// Create annotated tag with changelog content as message
	let tag_message = format!("Release {}\n\n{}", version, changelog_content.trim());

	let output = Command::new("git")
		.args(["tag", "-a", version, "-m", &tag_message])
		.output()?;

	if !output.status.success() {
		return Err(anyhow::anyhow!(
			"Failed to create tag: {}",
			String::from_utf8_lossy(&output.stderr)
		));
	}

	Ok(())
}
