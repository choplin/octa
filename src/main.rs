//! octa: GitHub-style Issue / Pull Request / Wiki collaboration, fully local.

mod app;
mod cli;
mod domain;
mod query;
mod sql;
mod store;
mod tui;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = cli::run(cli::parse().await).await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
