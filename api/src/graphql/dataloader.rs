//! Per-request batching [`DataLoader`](async_graphql::dataloader::DataLoader) shell.
//!
//! No entities to batch yet — steps 4/5 add `impl Loader<XId> for
//! DatabaseLoader<A>` blocks here (following seslogin's `graphql/dataloader.rs`
//! shape: a small `XId(pub ID)` newtype per entity, and a `load` that calls
//! `self.app.db().get_xs(...)` and zips the results back onto the requested keys).
//! [`get_dataloader`](super::get_dataloader) wires one of these into the request
//! context per request.

use std::sync::Arc;

use crate::app::{App, HasDb};

pub struct DatabaseLoader<A: App + HasDb + Send + Sync> {
    /// Read by the `impl Loader<...>` blocks steps 4/5 add.
    #[allow(dead_code)]
    app: Arc<A>,
}

impl<A: App + HasDb + Send + Sync> DatabaseLoader<A> {
    pub fn new(app: Arc<A>) -> Self {
        DatabaseLoader { app }
    }
}
