use super::Store;
use crate::domain::wiki::{WikiDetail, WikiPage};
use anyhow::Result;
impl Store {
    pub async fn create_wiki(&self, slug: &str, title: &str, body: &str) -> Result<String> {
        crate::app::wiki::create(&self.pool, self.repository_id()?, slug, title, body).await
    }
    pub async fn edit_wiki(
        &self,
        slug: &str,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<()> {
        crate::app::wiki::edit(&self.pool, self.repository_id()?, slug, title, body).await
    }
    pub async fn wiki_detail(&self, slug: &str) -> Result<WikiDetail> {
        crate::app::wiki::detail(&self.pool, self.repository_id()?, slug).await
    }
    pub async fn list_wiki(&self) -> Result<Vec<WikiPage>> {
        crate::app::wiki::list(
            &self.pool,
            (!self.is_all()).then(|| self.repository_id()).transpose()?,
        )
        .await
    }
}
