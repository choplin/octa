use super::Store;
use crate::domain::repo::Repo;
use anyhow::Result;
impl Store {
    /// Every repository in the store, regardless of the active scope.
    ///
    /// The listing is store-wide by definition, so it does not consult
    /// `repo_id`: the point of the command is to name repositories the caller
    /// is not currently inside.
    pub async fn list_repos(&self) -> Result<Vec<Repo>> {
        crate::app::repo::list(&self.pool).await
    }
}
