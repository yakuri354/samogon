use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub subcmd: Option<Subcmd>,
}

#[derive(Subcommand)]
pub enum Subcmd {
    // TODO Add subcommands
    Install { formulae: Vec<String> },
}
