#![warn(clippy::all, clippy::cargo, clippy::nursery, clippy::pedantic, clippy::absolute_paths)]
#![allow(clippy::multiple_crate_versions)]
#![deny(warnings)]

use anyhow::Result;
use clap::Parser;

mod cli;
mod config;
mod init;
mod login;
mod size;
mod sync;
mod track;
mod utils;

use crate::{
	cli::{Cli, Commands},
	config::Config,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let cli = Cli::parse();
	match cli.command {
		Commands::Init => init::run(),
		Commands::Login => login::run().await,
		Commands::Add { users, forks, frozen, submodules, no_submodules, no_sync, sync } => {
			let (repos, usernames): (Vec<String>, Vec<String>) =
				utils::expand_targets(users)?.into_iter().partition(|s| s.contains('/'));
			let submodules_override = if submodules {
				Some(true)
			} else if no_submodules {
				Some(false)
			} else {
				None
			};
			let config = Config::load()?;
			// --sync / --no-sync override the configured default in either direction;
			// with neither flag passed, fall back to the config's `no_sync` default.
			let no_sync = if sync {
				false
			} else if no_sync {
				true
			} else {
				config.no_sync
			};
			// Handle plain usernames first so that if someone mixes both formats
			// (e.g. `gitkeep add rust-lang rust-lang/mdBook`), the full-user tracking
			// wins and the individual pin is skipped cleanly.
			if !usernames.is_empty() {
				let client = config.build_client()?;
				let mut resolved = Vec::with_capacity(usernames.len());
				for name in &usernames {
					resolved.push(sync::resolve_login(&client, name).await?);
				}
				track::add(&resolved, forks, frozen, submodules_override)?;
				if !no_sync {
					let opts = sync::SyncOptions {
						force_forks: forks,
						force_submodules: submodules_override.unwrap_or(false),
						..Default::default()
					};
					sync::run_for(&resolved, opts).await?;
				}
			}
			if !repos.is_empty() {
				let client = config.build_client()?;
				let added = track::add_pinned(&repos, &client, submodules_override).await?;
				if !no_sync {
					if !added.restored_owners.is_empty() {
						sync::run_for(&added.restored_owners, sync::SyncOptions::default()).await?;
					}
					sync::run_pinned(&added.pinned).await?;
				}
			}
			Ok(())
		}
		Commands::Remove { users, delete, yes } => track::remove(&utils::expand_targets(users)?, delete, yes).await,
		Commands::List => track::list(),
		Commands::Size { format } => size::run(format),
		Commands::Sync { users, forks, submodules, pull_only, new_only, quiet, verbose } => {
			let verbosity = if quiet {
				sync::Verbosity::Quiet
			} else if verbose {
				sync::Verbosity::Verbose
			} else {
				sync::Verbosity::Normal
			};
			let opts = sync::SyncOptions { force_forks: forks, force_submodules: submodules, pull_only, new_only };
			sync::run(&users, opts, verbosity).await
		}
	}
}
