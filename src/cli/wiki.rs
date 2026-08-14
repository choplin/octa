use super::output::{Output, Tone};
use super::WikiCommand;
use crate::store::Store;
use anyhow::Result;
use urushi::View;
pub(crate) async fn run(store: &Store, command: WikiCommand) -> Result<()> {
    let output = Output::stdout();
    match command {
        WikiCommand::Create { title, slug, body } => {
            let slug = store
                .create_wiki(&slug.unwrap_or_else(|| title.clone()), &title, &body)
                .await?;
            output.print(View::line(
                output.line(Tone::Success, format!("created wiki page {slug}")),
            ))
        }
        WikiCommand::Set { slug, title, body } => {
            store
                .edit_wiki(&slug, title.as_deref(), body.as_deref())
                .await?;
            output.print(View::line(
                output.line(Tone::Success, format!("updated wiki page {slug}")),
            ))
        }
        WikiCommand::Show { slug, json } => {
            let detail = store.wiki_detail(&slug).await?;
            if json {
                println!("{}", serde_json::to_string(&detail)?)
            } else {
                let page = &detail.page;
                let mut view = View::line(output.row(&page.title, format!(" ({})", page.slug)));
                if !detail.links_to.is_empty() {
                    view = view.push(output.field("links to: ", detail.links_to.join(", ")));
                }
                if !detail.backlinks.is_empty() {
                    view = view.push(output.field("backlinks: ", detail.backlinks.join(", ")));
                }
                view = view.push(output.line(Tone::Body, "")).push(output.line(
                    if page.body.is_empty() {
                        Tone::Warning
                    } else {
                        Tone::Body
                    },
                    if page.body.is_empty() {
                        "(empty page)"
                    } else {
                        &page.body
                    },
                ));
                output.print(view)
            }
        }
        WikiCommand::List { json } => {
            let pages = store.list_wiki().await?;
            if json {
                println!("{}", serde_json::to_string(&pages)?)
            } else if pages.is_empty() {
                output.print(View::line(output.line(Tone::Warning, "no wiki pages")))
            } else {
                let rows = pages
                    .iter()
                    .map(|page| [page.slug.clone(), page.title.clone()]);
                output.print(output.table(["Slug", "Title"], rows))
            }
        }
    }
    Ok(())
}
