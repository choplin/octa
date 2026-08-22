//! Selection-aware, read-only GraphQL query surface.

mod model;
mod root;
mod sql;

use crate::store::Store;
use anyhow::{Context as _, Result};
use async_graphql::{EmptyMutation, EmptySubscription, Request, Response, Schema, Value};
use root::{QueryDb, QueryRoot};
use serde_json::Value as JsonValue;
use std::path::Path;
#[cfg(test)]
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub(super) const DEFAULT_LIMIT: i64 = 50;
pub(super) const MAX_LIMIT: i64 = 100;
const MAX_DEPTH: usize = 8;
const MAX_COMPLEXITY: usize = 500;

type QuerySchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

fn schema(store: &Store) -> Result<QuerySchema> {
    let db = QueryDb {
        pool: store.pool.clone(),
        repository: store.repository_id()?,
        accesses: Arc::new(AtomicUsize::new(0)),
        #[cfg(test)]
        statements: Arc::new(Mutex::new(Vec::new())),
    };
    Ok(Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(db)
        .limit_depth(MAX_DEPTH)
        .limit_complexity(MAX_COMPLEXITY)
        .finish())
}

pub fn schema_sdl(store: &Store) -> Result<String> {
    Ok(schema(store)?.sdl())
}

pub async fn execute(store: &Store, document: String, variables: Option<&str>) -> Result<Response> {
    let schema = schema(store)?;
    let mut request = Request::new(document);
    if let Some(raw) = variables {
        let json: JsonValue =
            serde_json::from_str(raw).context("--variables must be a JSON object")?;
        let object = json
            .as_object()
            .context("--variables must be a JSON object")?;
        request = request.variables(async_graphql::Variables::from_json(JsonValue::Object(
            object.clone(),
        )));
    }
    let mut response = schema.execute(request).await;
    let accesses = schema
        .data::<QueryDb>()
        .expect("query DB is schema data")
        .accesses
        .load(Ordering::Relaxed);
    response
        .extensions
        .insert("dbAccesses".to_string(), Value::from(accesses as i64));
    Ok(response)
}

pub fn read_document(file: Option<&Path>) -> Result<String> {
    match file {
        Some(path) => std::fs::read_to_string(path)
            .with_context(|| format!("cannot read GraphQL document {}", path.display())),
        None => {
            use std::io::Read;
            let mut document = String::new();
            std::io::stdin()
                .read_to_string(&mut document)
                .context("cannot read GraphQL document from stdin")?;
            Ok(document)
        }
    }
}
