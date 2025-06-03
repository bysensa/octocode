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

use clap::Args;
use parking_lot::RwLock;
use std::io::Write;
use std::sync::Arc;

use octocode::config::Config;
use octocode::indexer;
use octocode::state;
use octocode::store::Store;

#[derive(Args, Debug)]
pub struct IndexArgs {
	/// Skip git repository requirement and git-based optimizations
	#[arg(long)]
	pub no_git: bool,
}

pub async fn execute(
	store: &Store,
	config: &Config,
	args: &IndexArgs,
) -> Result<(), anyhow::Error> {
	let current_dir = std::env::current_dir()?;

	// Git repository validation and optimization
	let git_repo_root = if !args.no_git && config.index.require_git {
		// Check if we're in a git repository root
		if !indexer::git::is_git_repo_root(&current_dir) {
			return Err(anyhow::anyhow!(
				"❌ Error: Not in a git repository root!\n\n\
				This tool requires running from the root of a git repository.\n\
				Please:\n\
				1. Navigate to your git repository root (where .git/ folder exists)\n\
				2. Or use --no-git flag to skip git requirement\n\
				3. Or set index.require_git = false in your config"
			));
		}
		Some(current_dir.clone())
	} else if !args.no_git {
		// Try to find git root (for optimization even if not required)
		indexer::git::find_git_root(&current_dir)
	} else {
		None
	};

	if let Some(ref git_root) = git_repo_root {
		println!("✓ Git repository detected: {}", git_root.display());
	} else if args.no_git {
		println!("⚠️  Git integration disabled (--no-git flag)");
	} else {
		println!("⚠️  No git repository found, using file-based indexing");
	}

	let state = state::create_shared_state();
	state.write().current_directory = current_dir;

	// Spawn the progress display task
	let progress_handle = tokio::spawn(display_indexing_progress(state.clone()));

	// Start indexing with git optimization
	indexer::index_files(store, state.clone(), config, git_repo_root.as_deref()).await?;

	// Wait for the progress display to finish
	let _ = progress_handle.await;

	// Flush index to disk
	store.flush().await?;
	Ok(())
}

pub async fn display_indexing_progress(state: Arc<RwLock<state::IndexState>>) {
	let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
	let mut spinner_idx = 0;
	let mut last_indexed = 0;
	let mut last_graphrag_blocks = 0;
	let mut last_status_message = String::new();
	let mut indexing_complete = false;

	while !indexing_complete {
		// Gather all necessary state in local variables before the await
		let current_indexed;
		let total_files;
		let graphrag_blocks;
		let status_message;
		let graphrag_enabled;
		let counting_files;

		{
			let current_state = state.read();
			current_indexed = current_state.indexed_files;
			total_files = current_state.total_files;
			graphrag_blocks = current_state.graphrag_blocks;
			status_message = current_state.status_message.clone();
			graphrag_enabled = current_state.graphrag_enabled;
			counting_files = current_state.counting_files;
			indexing_complete = current_state.indexing_complete; // Update our loop control variable
			                                            // Lock is dropped here when we exit the scope
		}

		// Exit early if indexing is complete
		if indexing_complete {
			break;
		}

		// Only redraw if something changed or on spinner change
		if current_indexed != last_indexed
			|| graphrag_blocks != last_graphrag_blocks
			|| status_message != last_status_message
		{
			// Clear the line and move cursor to beginning with \r
			print!("\r\x1b[K"); // \x1b[K clears the rest of the line

			// Build display string based on current phase
			if counting_files {
				print!("{} Counting files...", spinner_chars[spinner_idx]);
			} else if total_files > 0 {
				let percentage = (current_indexed as f32 / total_files as f32 * 100.0) as u32;
				print!(
					"{} Indexing: {}/{} files ({}%)",
					spinner_chars[spinner_idx], current_indexed, total_files, percentage
				);

				// Add GraphRAG info if enabled and blocks exist
				if graphrag_enabled && graphrag_blocks > 0 {
					print!(", GraphRAG: {} blocks", graphrag_blocks);
				}
			} else {
				// Fallback for when total is not known yet
				print!(
					"{} Indexing: {} files",
					spinner_chars[spinner_idx], current_indexed
				);
			}

			// Add status message if there is one
			if !status_message.is_empty() {
				print!(" - {}", status_message);
			}

			std::io::stdout().flush().unwrap();
			last_indexed = current_indexed;
			last_graphrag_blocks = graphrag_blocks;
			last_status_message = status_message.clone();
		} else {
			// Just update the spinner
			print!("\r\x1b[K"); // Clear the line
			if counting_files {
				print!("{} Counting files...", spinner_chars[spinner_idx]);
			} else if total_files > 0 {
				let percentage = (current_indexed as f32 / total_files as f32 * 100.0) as u32;
				print!(
					"{} Indexing: {}/{} files ({}%)",
					spinner_chars[spinner_idx], current_indexed, total_files, percentage
				);

				// Add GraphRAG info if enabled and blocks exist
				if graphrag_enabled && graphrag_blocks > 0 {
					print!(", GraphRAG: {} blocks", graphrag_blocks);
				}
			} else {
				print!(
					"{} Indexing: {} files",
					spinner_chars[spinner_idx], current_indexed
				);
			}

			// Add status message if there is one
			if !status_message.is_empty() {
				print!(" - {}", status_message);
			}
			std::io::stdout().flush().unwrap();
		}

		spinner_idx = (spinner_idx + 1) % spinner_chars.len();
		tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
	}

	// Final summary message
	let final_indexed;
	let final_total;
	let final_graphrag_enabled;
	let final_graphrag_blocks;

	{
		let final_state = state.read();
		final_indexed = final_state.indexed_files;
		final_total = final_state.total_files;
		final_graphrag_enabled = final_state.graphrag_enabled;
		final_graphrag_blocks = final_state.graphrag_blocks;
	}

	print!("\r\x1b[K"); // Clear the line before final message
	if !final_graphrag_enabled {
		println!(
			"✓ Indexing complete! {} of {} files processed",
			final_indexed, final_total
		);
	} else {
		println!(
			"✓ Indexing complete! {} of {} files processed, GraphRAG: {} blocks",
			final_indexed, final_total, final_graphrag_blocks
		);
	}
}
