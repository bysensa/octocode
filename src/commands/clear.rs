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

use octocode::store::Store;

/// Clear all database tables and drop schema
pub async fn execute(store: &Store) -> Result<(), anyhow::Error> {
	println!("Clearing all database tables and dropping schemas...");
	store.clear_all_tables().await?;
	println!("Successfully dropped all tables and schemas.");
	println!("Note: Tables will be recreated with current schema on next indexing operation.");
	Ok(())
}
