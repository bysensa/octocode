use clap::Args;
use octocode::config::Config;
use octocode::store::Store;

#[derive(Args, Debug)]
pub struct DebugArgs {
	/// List all files currently indexed in the database
	#[arg(long)]
	pub list_files: bool,
}

pub async fn execute(store: &Store, _config: &Config, args: &DebugArgs) -> Result<(), anyhow::Error> {
	if args.list_files {
		println!("Listing all files currently indexed in the database...");
		store.debug_list_all_files().await?;
	} else {
		println!("Debug options:");
		println!("  --list-files    List all files currently indexed in the database");
	}

	Ok(())
}
