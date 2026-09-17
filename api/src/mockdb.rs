//! In-process implementation of [`crate::db::Handler`] that fails every method.
//!
//! This is *not* a lightweight substitute for a real database — its job is
//! exercising error paths (what happens when the DB call in the middle of a
//! resolver fails?), not standing in for one. Tests that need actual data need a
//! real DynamoDB (Local or otherwise); `bin/export-schema` and
//! `tests/graphql_error_codes.rs` use this because they only need *a* type that
//! implements [`crate::db::Handler`], never because they read or write through it.

use crate::db;

#[derive(Debug, Default, Clone, Copy)]
pub struct Handler;

impl Handler {
    pub fn new() -> Self {
        Self
    }

    /// Every method added here (starting in step 3) should route through this
    /// rather than inventing its own error, so a mockdb failure always reads the
    /// same way regardless of which method produced it.
    #[allow(dead_code)] // used once step 3 adds the first db::Handler method
    fn unsupported<T>() -> db::Result<T> {
        Err(db::Error::Infrastructure(
            "mockdb operation not implemented".to_string(),
        ))
    }
}

impl db::Handler for Handler {}
