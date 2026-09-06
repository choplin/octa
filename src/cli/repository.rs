use super::output::Output;
use super::RepositoryCommand;
use crate::domain::repository::Repository;
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
                        repository.updated_at.clone(),
                        repository.open_issues.to_string(),
                        repository.in_progress_issues.to_string(),
                    ]
                });
                output.print(output.table(
                    ["Name", "Path", "Created", "Updated", "Open", "In Progress"],
                    rows,
                ));
            }
        }
        RepositoryCommand::Register { name, path, json } => {
            let repository = store.register_repository(&name, path.as_deref()).await?;
            print_repository(&output, &repository, json)?;
        }
        RepositoryCommand::Set { target, name, json } => {
            let repository = store.set_repository_name(&target, &name).await?;
            print_repository(&output, &repository, json)?;
        }
        RepositoryCommand::Relocate { name, path, json } => {
            let repository = store.relocate_repository(&name, path.as_deref()).await?;
            print_repository(&output, &repository, json)?;
        }
    }
    Ok(())
}

fn print_repository(output: &Output, repository: &Repository, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(repository)?);
    } else {
        output.print_lines([
            output.field("Name", repository.name.clone()),
            output.field("Path", repository.path.clone()),
        ]);
    }
    Ok(())
}
