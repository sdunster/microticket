//! GraphQL schema root.
//!
//! Minimal and empty-but-valid for now: just enough for `bin/export-schema` to
//! run and `api/schema.graphql` to be generated and committed. `QueryRoot`,
//! `MutationRoot`, error-code extensions, auth guards, pagination helpers and
//! dataloaders all land in the API foundations step.

use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema};

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// API build version — the git commit this server was built from.
    async fn version(&self) -> String {
        crate::environment::GIT_REV.to_string()
    }
}

pub type MicroticketSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub fn build_schema() -> MicroticketSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription).finish()
}
