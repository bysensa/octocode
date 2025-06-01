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

use octocode::config::Config;
use octocode::store::Store;
use octocode::state;
use octocode::indexer;

use super::index::IndexArgs;

#[derive(Args, Debug)]
pub struct WatchArgs {
	/// Run in quiet mode with less output
	#[arg(long, short)]
	pub quiet: bool,

	/// Change debounce time in seconds
	#[arg(long, short)]
	pub debounce: Option<u64>,

	/// Skip git repository requirement and git-based optimizations
	#[arg(long)]
	pub no_git: bool,
}

pub async fn execute(store: &Store, config: &Config, args: &WatchArgs) -> Result<(), anyhow::Error> {
	let current_dir = std::env::current_dir()?;

	// Only show verbose output if not in quiet mode
	if !args.quiet {
		println!("Starting watch mode for current directory: {}", current_dir.display());
		println!("Initial indexing...");
	}

	// Do initial indexing
	if !args.quiet {
		// If not in quiet mode, use the regular indexing with progress display
		super::index::execute(store, config, &IndexArgs { no_git: args.no_git }).await?
	} else {
		// In quiet mode, just do the indexing without progress display
		let state = state::create_shared_state();
		state.write().current_directory = current_dir.clone();

		// Get git root for optimization
		let git_repo_root = if !args.no_git {
			indexer::git::find_git_root(&current_dir)
		} else {
			None
		};

		indexer::index_files(store, state.clone(), config, git_repo_root.as_deref()).await?;
	}

	if !args.quiet {
		println!("Watching for changes (press Ctrl+C to stop)...");
	}

	// Setup the file watcher with debouncer
	use notify_debouncer_mini::{new_debouncer, DebouncedEvent};
	use std::sync::mpsc::channel;
	use std::time::Duration;

	let (tx, rx) = channel();

	// Get the debounce time from args or use default
	let debounce_secs = args.debounce.unwrap_or(2);

	// Copy quiet flag to capture in closure
	let quiet_mode = args.quiet;

	// Create a debounced watcher to call our tx sender when files change
	let mut debouncer = new_debouncer(
		Duration::from_secs(debounce_secs),
		move |res: Result<Vec<DebouncedEvent>, notify::Error>| {
			match res {
				Ok(events) => {
					// Filter out events from .octocode directory to prevent reindexing loops
					let relevant_events = events.iter().filter(|event| {
						let path = event.path.to_string_lossy();
						!path.contains(".octocode") && !path.contains("target/") && !path.contains(".git/")
					}).count();

					if relevant_events > 0 {
						let _ = tx.send(());
					}
				},
				Err(e) => {
					if !quiet_mode {
						eprintln!("Error in file watcher: {:?}", e);
					}
				},
			}
		},
	)?;

	// Add the current directory to the watcher
	debouncer.watcher().watch(&current_dir, notify::RecursiveMode::Recursive)?;

	// Create shared state for reindexing
	let state = state::create_shared_state();
	state.write().current_directory = current_dir;

	// Keep a copy of the config for reindexing
	let config = config.clone();

	loop {
		// Wait for changes
		match rx.recv() {
			Ok(()) => {
				if !args.quiet {
					println!("\nDetected file changes, reindexing...");
				}

				// Reset the indexing state
				{
					let mut state_guard = state.write();
					state_guard.indexed_files = 0;
					state_guard.indexing_complete = false;
				}

				// Reindex the codebase
				tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // Give a bit of time for all file changes to complete

				if !args.quiet {
					// Use regular indexing with progress in non-quiet mode
					super::index::execute(store, &config, &IndexArgs { no_git: args.no_git }).await?
				} else {
					// In quiet mode, just do the indexing without progress display
					let git_repo_root = if !args.no_git {
						indexer::git::find_git_root(&state.read().current_directory)
					} else {
						None
					};
					indexer::index_files(store, state.clone(), &config, git_repo_root.as_deref()).await?;
				}
			},
			Err(e) => {
				if !args.quiet {
					eprintln!("Watch error: {:?}", e);
				}
				break;
			}
		}
	}

	Ok(())
}