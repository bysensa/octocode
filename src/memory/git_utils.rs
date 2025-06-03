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

use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Utilities for Git operations
pub struct GitUtils;

impl GitUtils {
	/// Get the current Git commit hash
	pub fn get_current_commit() -> Option<String> {
		let output = Command::new("git")
			.args(["rev-parse", "HEAD"])
			.output()
			.ok()?;

		if output.status.success() {
			let commit = String::from_utf8(output.stdout).ok()?;
			Some(commit.trim().to_string())
		} else {
			None
		}
	}

	/// Get the short Git commit hash (7 characters)
	pub fn get_current_commit_short() -> Option<String> {
		let output = Command::new("git")
			.args(["rev-parse", "--short", "HEAD"])
			.output()
			.ok()?;

		if output.status.success() {
			let commit = String::from_utf8(output.stdout).ok()?;
			Some(commit.trim().to_string())
		} else {
			None
		}
	}

	/// Check if the current directory is a Git repository
	pub fn is_git_repository() -> bool {
		Command::new("git")
			.args(["rev-parse", "--git-dir"])
			.output()
			.map(|output| output.status.success())
			.unwrap_or(false)
	}

	/// Get the Git repository root directory
	pub fn get_repository_root() -> Option<String> {
		let output = Command::new("git")
			.args(["rev-parse", "--show-toplevel"])
			.output()
			.ok()?;

		if output.status.success() {
			let root = String::from_utf8(output.stdout).ok()?;
			Some(root.trim().to_string())
		} else {
			None
		}
	}

	/// Get the current branch name
	pub fn get_current_branch() -> Option<String> {
		let output = Command::new("git")
			.args(["branch", "--show-current"])
			.output()
			.ok()?;

		if output.status.success() {
			let branch = String::from_utf8(output.stdout).ok()?;
			Some(branch.trim().to_string())
		} else {
			None
		}
	}

	/// Get files modified in the current working directory
	pub fn get_modified_files() -> Result<Vec<String>> {
		let output = Command::new("git")
			.args(["diff", "--name-only", "HEAD"])
			.output()?;

		if output.status.success() {
			let files_str = String::from_utf8(output.stdout)?;
			let files: Vec<String> = files_str
				.lines()
				.filter(|line| !line.trim().is_empty())
				.map(|line| line.trim().to_string())
				.collect();
			Ok(files)
		} else {
			Ok(Vec::new())
		}
	}

	/// Get files changed between two commits
	pub fn get_changed_files_between_commits(
		from_commit: &str,
		to_commit: &str,
	) -> Result<Vec<String>> {
		let output = Command::new("git")
			.args(["diff", "--name-only", from_commit, to_commit])
			.output()?;

		if output.status.success() {
			let files_str = String::from_utf8(output.stdout)?;
			let files: Vec<String> = files_str
				.lines()
				.filter(|line| !line.trim().is_empty())
				.map(|line| line.trim().to_string())
				.collect();
			Ok(files)
		} else {
			Ok(Vec::new())
		}
	}

	/// Get commit information for a specific commit
	pub fn get_commit_info(commit_hash: &str) -> Option<CommitInfo> {
		let output = Command::new("git")
			.args([
				"show",
				"--format=%H|%h|%an|%ae|%at|%s",
				"--no-patch",
				commit_hash,
			])
			.output()
			.ok()?;

		if output.status.success() {
			let info_str = String::from_utf8(output.stdout).ok()?;
			let parts: Vec<&str> = info_str.trim().split('|').collect();
			if parts.len() >= 6 {
				return Some(CommitInfo {
					full_hash: parts[0].to_string(),
					short_hash: parts[1].to_string(),
					author_name: parts[2].to_string(),
					author_email: parts[3].to_string(),
					timestamp: parts[4].parse().unwrap_or(0),
					message: parts[5].to_string(),
				});
			}
		}
		None
	}

	/// Check if a file is tracked by Git
	pub fn is_file_tracked<P: AsRef<Path>>(file_path: P) -> bool {
		Command::new("git")
			.args(["ls-files", "--error-unmatch"])
			.arg(file_path.as_ref())
			.output()
			.map(|output| output.status.success())
			.unwrap_or(false)
	}

	/// Get the relative path from repository root
	pub fn get_relative_path<P: AsRef<Path>>(file_path: P) -> Option<String> {
		if let Some(repo_root) = Self::get_repository_root() {
			if let Ok(absolute_path) = file_path.as_ref().canonicalize() {
				if let Ok(relative) = absolute_path.strip_prefix(&repo_root) {
					return Some(relative.to_string_lossy().to_string());
				}
			}
		}
		file_path.as_ref().to_str().map(|s| s.to_string())
	}

	/// Get commits that modified a specific file
	pub fn get_file_commit_history<P: AsRef<Path>>(
		file_path: P,
		limit: Option<usize>,
	) -> Result<Vec<String>> {
		let mut args = vec!["log", "--format=%H", "--follow"];

		let limit_str;
		if let Some(limit) = limit {
			args.push("-n");
			limit_str = limit.to_string();
			args.push(&limit_str);
		}

		args.push("--");

		let path_str = file_path.as_ref().to_str().unwrap_or("");
		args.push(path_str);

		let output = Command::new("git").args(&args).output()?;

		if output.status.success() {
			let commits_str = String::from_utf8(output.stdout)?;
			let commits: Vec<String> = commits_str
				.lines()
				.filter(|line| !line.trim().is_empty())
				.map(|line| line.trim().to_string())
				.collect();
			Ok(commits)
		} else {
			Ok(Vec::new())
		}
	}

	/// Get the last commit that modified a file
	pub fn get_file_last_commit<P: AsRef<Path>>(file_path: P) -> Option<String> {
		Self::get_file_commit_history(file_path, Some(1))
			.ok()?
			.into_iter()
			.next()
	}
}

/// Information about a Git commit
#[derive(Debug, Clone)]
pub struct CommitInfo {
	pub full_hash: String,
	pub short_hash: String,
	pub author_name: String,
	pub author_email: String,
	pub timestamp: i64,
	pub message: String,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_git_operations() {
		// These tests will only pass in a Git repository
		if GitUtils::is_git_repository() {
			// Test basic Git operations
			assert!(GitUtils::get_current_commit().is_some());
			assert!(GitUtils::get_current_commit_short().is_some());
			assert!(GitUtils::get_repository_root().is_some());

			// Test current branch (might be None in detached HEAD state)
			let _branch = GitUtils::get_current_branch();

			// Test modified files (should not fail even if empty)
			assert!(GitUtils::get_modified_files().is_ok());
		}
	}
}
