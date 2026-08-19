use super::output::Output;
use super::RepoCommand;
use crate::store::Store;
use anyhow::Result;

pub(crate) async fn run(store: &Store, command: RepoCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        RepoCommand::List { json } => {
            let repos = store.list_repos().await?;
            if json {
                println!("{}", serde_json::to_string(&repos)?)
            } else {
                let rows = repos.iter().map(|repo| {
                    [
                        repo.name.clone(),
                        repo.path.clone(),
                        repo.created_at.clone(),
                        repo.open_issues.to_string(),
                        repo.in_progress_issues.to_string(),
                    ]
                });
                output
                    .print(output.table(["Name", "Path", "Created", "Open", "In Progress"], rows));
            }
        }
    }
    Ok(())
}
