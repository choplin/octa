use super::WikiCommand;
use crate::store::Store;
use anyhow::Result;
pub(crate) async fn run(store: &Store, command: WikiCommand) -> Result<()> {
    match command {
        WikiCommand::Create { title, slug, body } => {
            let slug = store
                .create_wiki(&slug.unwrap_or_else(|| title.clone()), &title, &body)
                .await?;
            println!("created wiki page {slug}")
        }
        WikiCommand::Edit { slug, title, body } => {
            store
                .edit_wiki(&slug, title.as_deref(), body.as_deref())
                .await?;
            println!("updated wiki page {slug}")
        }
        WikiCommand::Show { slug, json } => {
            let detail = store.wiki_detail(&slug).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?)
            } else {
                let page = &detail.page;
                println!("{} ({})", page.title, page.slug);
                if !detail.links_to.is_empty() {
                    println!("links to: {}", detail.links_to.join(", "));
                }
                if !detail.backlinks.is_empty() {
                    println!("backlinks: {}", detail.backlinks.join(", "));
                }
                println!();
                println!(
                    "{}",
                    if page.body.is_empty() {
                        "(empty page)"
                    } else {
                        &page.body
                    }
                );
            }
        }
        WikiCommand::List { json } => {
            let pages = store.list_wiki().await?;
            if json {
                println!("{}", serde_json::to_string(&pages)?)
            } else if pages.is_empty() {
                println!("no wiki pages")
            } else {
                for page in pages {
                    println!("{:<24} {}", page.slug, page.title)
                }
            }
        }
    }
    Ok(())
}
