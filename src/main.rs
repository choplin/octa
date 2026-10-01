//! octa: Local Issue collaboration for developers and AI agents.

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
