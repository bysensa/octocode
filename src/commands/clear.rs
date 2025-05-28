use octocode::store::Store;

/// Clear all database tables and drop schema
pub async fn execute(store: &Store) -> Result<(), anyhow::Error> {
	println!("Clearing all database tables and dropping schemas...");
	store.clear_all_tables().await?;
	println!("Successfully dropped all tables and schemas.");
	println!("Note: Tables will be recreated with current schema on next indexing operation.");
	Ok(())
}
