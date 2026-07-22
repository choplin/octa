//! octa: GitHub-style Issue / Pull Request / Wiki collaboration, fully local.

mod app;
mod cli;
mod domain;
mod sql;
mod store;

use clap::Parser;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = cli::run(cli::Cli::parse()).await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
