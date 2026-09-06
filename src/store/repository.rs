use super::Store;
use crate::domain::repository::Repository;
use anyhow::{Context, Result};
use std::path::Path;

impl Store {
    /// Every repository in the store, regardless of the active scope.
    ///
    /// The listing is store-wide by definition, so it does not consult
    /// `repository_id`: the point of the command is to name repositories the caller
    /// is not currently inside.
    pub async fn list_repositories(&self) -> Result<Vec<Repository>> {
        crate::app::repository::list(&self.pool).await
    }

    /// Explicitly register a Git repository under a unique user-facing name.
    pub async fn register_repository(&self, name: &str, path: Option<&Path>) -> Result<Repository> {
        let directory = path
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);
        let repository = super::resolve_repository_identity_at(&directory)?;
        self.register_resolved(repository, Some(name)).await?;
        self.repository_named(name).await
    }

    /// Set the stable user-facing name of a repository.
    pub async fn set_repository_name(&self, name: &str, new_name: &str) -> Result<Repository> {
        let identity = crate::app::repository::identity_by_name(&self.pool, name).await?;
        crate::app::repository::validate_name(&self.pool, identity.id, new_name).await?;
        let repository = super::resolve_repository_identity_at(Path::new(&identity.path))?;
        let configured_name = repository.configured_name.context(format!(
            "Git repository at {:?} has no {}",
            repository.path,
            super::REPOSITORY_NAME_CONFIG
        ))?;
        if configured_name != identity.name {
            anyhow::bail!(
                "repository name mismatch for {name:?}; Git config contains {configured_name:?}"
            );
        }
        super::write_repository_name(&repository.command_directory, new_name)?;
        if let Err(error) = crate::app::repository::set_name(
            &self.pool,
            identity.id,
            &identity.name,
            &identity.path,
            new_name,
        )
        .await
        {
            let stored = crate::app::repository::identity_by_path(&self.pool, &repository.path)
                .await?
                .map(|stored| stored.name);
            super::restore_repository_name(
                &repository.command_directory,
                new_name,
                stored.as_deref().or(Some(&identity.name)),
            )
            .with_context(|| format!("repository name update failed before cleanup: {error}"))?;
            return Err(error);
        }
        self.repository_named(new_name).await
    }

    /// Rebind a registered repository after verifying its configured name.
    pub async fn relocate_repository(&self, name: &str, path: Option<&Path>) -> Result<Repository> {
        let directory = path
            .map(Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);
        let repository = super::resolve_repository_identity_at(&directory)?;
        let configured_name = repository.configured_name.context(format!(
            "Git repository at {:?} has no {}",
            repository.path,
            super::REPOSITORY_NAME_CONFIG
        ))?;
        crate::app::repository::relocate(&self.pool, name, &repository.path, &configured_name)
            .await?;
        self.repository_named(name).await
    }

    async fn repository_named(&self, name: &str) -> Result<Repository> {
        self.list_repositories()
            .await?
            .into_iter()
            .find(|repository| repository.name == name)
            .context("updated repository could not be read back")
    }
}
