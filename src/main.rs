mod args;
mod config;
mod database;
mod deps;
mod fetch_install;
mod platform;
mod repo;
mod ui;
mod util;

use std::process::exit;

use anyhow::Result;
use clap::Parser;
use ui::confirm_install;

#[tokio::main]
async fn main() -> Result<()> {
    // std::env::set_var("RUST_BACKTRACE", "1");

    let args = args::Args::try_parse()?;

    match args.subcmd {
        Some(args::Subcmd::Install { formulae }) => {
            let repo = repo::get_repo().await?;
            let deps = deps::find_deps(&formulae, &repo)?;
            let deps_formulae = deps
                .iter()
                .map(|d| repo.formulae[d].clone())
                .collect::<Vec<_>>();

            if confirm_install(&deps_formulae).await? {
                fetch_install::stream_all(deps_formulae).await?;
            } else {
                println!("! aborted");
                exit(1);
            }
        }
        None => {
            return Ok(());
        }
    }

    Ok(())
}
