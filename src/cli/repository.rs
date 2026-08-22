use super::output::Output;
use super::RepositoryCommand;
use crate::store::Store;
use anyhow::Result;

pub(crate) async fn run(store: &Store, command: RepositoryCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        RepositoryCommand::List { json } => {
            let repositories = store.list_repositories().await?;
            if json {
                println!("{}", serde_json::to_string(&repositories)?)
            } else {
                let rows = repositories.iter().map(|repository| {
                    [
                        repository.name.clone(),
                        repository.path.clone(),
                        repository.created_at.clone(),
                        repository.open_issues.to_string(),
                        repository.in_progress_issues.to_string(),
                    ]
                });
                output
                    .print(output.table(["Name", "Path", "Created", "Open", "In Progress"], rows));
            }
        }
    }
    Ok(())
}
