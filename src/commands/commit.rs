use clap::Args;
use std::process::Command;
use std::io::{self, Write};
use anyhow::Result;

use octocode::config::Config;

#[derive(Args, Debug)]
pub struct CommitArgs {
	/// Add all changes before committing
	#[arg(short, long)]
	pub all: bool,

	/// Custom commit message (skips LLM generation)
	#[arg(short, long)]
	pub message: Option<String>,

	/// Skip confirmation prompt
	#[arg(short, long)]
	pub yes: bool,
}

pub async fn execute(config: &Config, args: &CommitArgs) -> Result<()> {
	let current_dir = std::env::current_dir()?;
	
	// Check if we're in a git repository
	if !current_dir.join(".git").exists() {
		return Err(anyhow::anyhow!("❌ Not in a git repository!"));
	}

	// Add all files if requested
	if args.all {
		println!("📂 Adding all changes...");
		let output = Command::new("git")
			.args(&["add", "."])
			.current_dir(&current_dir)
			.output()?;

		if !output.status.success() {
			return Err(anyhow::anyhow!("Failed to add files: {}", 
				String::from_utf8_lossy(&output.stderr)));
		}
	}

	// Check if there are staged changes
	let output = Command::new("git")
		.args(&["diff", "--cached", "--name-only"])
		.current_dir(&current_dir)
		.output()?;

	if !output.status.success() {
		return Err(anyhow::anyhow!("Failed to check staged changes: {}", 
			String::from_utf8_lossy(&output.stderr)));
	}

	let staged_files = String::from_utf8(output.stdout)?;
	if staged_files.trim().is_empty() {
		return Err(anyhow::anyhow!("❌ No staged changes to commit. Use 'git add' or --all flag."));
	}

	println!("📋 Staged files:");
	for file in staged_files.lines() {
		println!("  • {}", file);
	}

	// Generate commit message
	let commit_message = if let Some(message) = &args.message {
		message.clone()
	} else {
		println!("\n🤖 Generating commit message...");
		generate_commit_message(&current_dir, config).await?
	};

	println!("\n📝 Generated commit message:");
	println!("═══════════════════════════════════");
	println!("{}", commit_message);
	println!("═══════════════════════════════════");

	// Confirm with user (unless --yes flag is used)
	if !args.yes {
		print!("\nProceed with this commit? [y/N] ");
		io::stdout().flush()?;
		
		let mut input = String::new();
		io::stdin().read_line(&mut input)?;
		
		if !input.trim().to_lowercase().starts_with('y') {
			println!("❌ Commit cancelled.");
			return Ok(());
		}
	}

	// Perform the commit
	println!("💾 Committing changes...");
	let output = Command::new("git")
		.args(&["commit", "-m", &commit_message])
		.current_dir(&current_dir)
		.output()?;

	if !output.status.success() {
		return Err(anyhow::anyhow!("Failed to commit: {}", 
			String::from_utf8_lossy(&output.stderr)));
	}

	println!("✅ Successfully committed changes!");
	
	// Show commit info
	let output = Command::new("git")
		.args(&["log", "--oneline", "-1"])
		.current_dir(&current_dir)
		.output()?;

	if output.status.success() {
		let commit_info = String::from_utf8_lossy(&output.stdout);
		println!("📄 Commit: {}", commit_info.trim());
	}

	Ok(())
}

async fn generate_commit_message(repo_path: &std::path::Path, config: &Config) -> Result<String> {
	// Get the diff of staged changes
	let output = Command::new("git")
		.args(&["diff", "--cached"])
		.current_dir(repo_path)
		.output()?;

	if !output.status.success() {
		return Err(anyhow::anyhow!("Failed to get diff: {}", 
			String::from_utf8_lossy(&output.stderr)));
	}

	let diff = String::from_utf8(output.stdout)?;
	
	if diff.trim().is_empty() {
		return Err(anyhow::anyhow!("No staged changes found"));
	}

	// Prepare the prompt for the LLM
	let prompt = format!(
		"You are a Git commit message generator. Analyze the following git diff and generate a concise, \
		descriptive commit message following conventional commit format.\n\n\
		Rules:\n\
		1. Use conventional commit format: type(scope): description\n\
		2. Types: feat, fix, docs, style, refactor, test, chore\n\
		3. Keep the description under 50 characters\n\
		4. Focus on WHAT changed, not HOW\n\
		5. Use imperative mood (\"add\" not \"added\")\n\
		6. Don't include file names unless critical\n\n\
		Git diff:\n\
		```\n{}\n```\n\n\
		Generate only the commit message, nothing else:",
		// Truncate diff if it's too long (keep first 3000 chars)
		if diff.len() > 3000 {
			format!("{}...\n[diff truncated for brevity]", &diff[..3000])
		} else {
			diff
		}
	);

	// Call the LLM using existing infrastructure
	match call_llm_for_commit_message(&prompt, config).await {
		Ok(message) => {
			// Clean up the response
			let cleaned = message
				.trim()
				.lines()
				.next() // Take only the first line
				.unwrap_or("chore: update files")
				.trim_matches('"') // Remove quotes if present
				.trim();
			
			// Validate the message
			if cleaned.is_empty() {
				Ok("chore: update files".to_string())
			} else if cleaned.len() > 72 {
				// Truncate if too long
				Ok(format!("{}...", &cleaned[..69]))
			} else {
				Ok(cleaned.to_string())
			}
		},
		Err(e) => {
			eprintln!("Warning: LLM call failed ({}), using fallback", e);
			Ok("chore: update files".to_string())
		}
	}
}

async fn call_llm_for_commit_message(prompt: &str, config: &Config) -> Result<String> {
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
		"max_tokens": 100
	});

	let response = client
		.post(&format!("{}/chat/completions", config.openrouter.base_url.trim_end_matches('/')))
		.header("Authorization", format!("Bearer {}", api_key))
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