//! The database abstraction: [`Handler`] trait, [`Error`], and the row/paging types
//! every backend and every later domain module builds on.
//!
//! Deliberately near-empty right now — this is the API-foundations step, and there
//! are no domain entities yet. Step 3 adds the auth tables (`login_code`,
//! `user_token`, `webauthn_credential`, `ephemeral_state`); step 4 adds
//! `instance`/`membership`; step 5 adds `ticket`/`ticket_message`/`counter`. Each of
//! those steps grows [`Handler`] with the methods it needs, following the shape of
//! [`crate::dynamodb::Handler`]'s generic infrastructure (already in place) and
//! [`crate::mockdb::Handler`]'s all-fail mock.

use thiserror::Error;

/// These errors are separated into groups because callers want to handle them
/// differently (e.g. `NotFound` from a lookup is often fine to surface to the user;
/// `Infrastructure` usually isn't).
#[derive(Error, Debug)]
pub enum Error {
    /// Returned when a queried record does not exist.
    #[error("Record not found: {0}")]
    NotFound(String),
    /// Returned when a DB row cannot be deserialized into the expected type.
    #[error("Hydration error: {0}")]
    Hydration(String),
    /// An unexpected error, probably fine to ignore and retry.
    #[error("Infrastructure error: {0}")]
    Infrastructure(String),
    /// Returned when a row violates an expected data-integrity invariant.
    #[error("Data integrity error: {0}")]
    Integrity(String),
    /// Type conversion error, e.g. converting a string ID to an integer.
    #[error("Data type conversion error: {0}")]
    TypeConversion(String),
    #[error("Mutation disabled")]
    MutationDisabled,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Collapse the results of a lookup on an attribute that is *expected* to be unique
/// (but not enforced as unique by the data model) down to at most one row. Returns
/// an [`Error::Integrity`] if more than one row shares the attribute, so callers
/// that assume uniqueness fail loudly rather than silently picking an arbitrary
/// match.
pub fn at_most_one<T>(mut matches: Vec<T>, describe: impl FnOnce() -> String) -> Result<Option<T>> {
    if matches.len() > 1 {
        return Err(Error::Integrity(describe()));
    }
    Ok(matches.pop())
}

/// Implemented by every domain row type so generic helpers like
/// [`crate::dynamodb::Handler::get_records`] can index results by primary key.
pub trait HasID {
    fn id(&self) -> &str;
}

/// Where a table scan left off. Scanning the base table returns a
/// `LastEvaluatedKey` of just the primary key, and every scannable table is
/// hash-keyed on `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanCursor {
    pub last_id: String,
}

/// One page of a table scan.
///
/// Rows are hydrated independently: an `Err` — always [`Error::Hydration`], naming
/// the offending row — is one bad record, not a failed page, so a scan can survey a
/// table end to end and report every problem it finds.
///
/// **`rows` being empty does not mean the scan is finished.** DynamoDB's `Limit`
/// counts items *examined*, and a page can come back empty while more remain. Only
/// `next == None` ends the walk.
#[derive(Debug)]
pub struct ScanPage<T> {
    pub rows: Vec<Result<T>>,
    pub next: Option<ScanCursor>,
}

/// `Sync` is required so a `&impl Handler` (including the erased handle returned by
/// [`crate::app::HasDb::db`]) can be held across `.await` inside the `Send` futures
/// the GraphQL/Poem stack builds. Both implementors ([`crate::dynamodb::Handler`],
/// [`crate::mockdb::Handler`]) are already `Sync`.
///
/// Intentionally empty for now — see the module doc for which step adds which
/// methods. Every method added here should follow the RPITIT style already used by
/// [`crate::mail::Handler`]: `fn foo(&self, ...) -> impl Future<Output = Result<T>> + Send`,
/// not `async_trait`.
pub trait Handler: Sync {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_most_one_returns_none_for_no_matches() {
        assert_eq!(
            at_most_one::<i32>(vec![], || "none".to_string()).unwrap(),
            None
        );
    }

    #[test]
    fn at_most_one_returns_the_single_match() {
        assert_eq!(
            at_most_one(vec![42], || "one".to_string()).unwrap(),
            Some(42)
        );
    }

    #[test]
    fn at_most_one_errors_on_multiple_matches() {
        let err = at_most_one(vec![1, 2], || "dup email".to_string()).unwrap_err();
        match err {
            Error::Integrity(msg) => assert_eq!(msg, "dup email"),
            other => panic!("expected Integrity, got {other:?}"),
        }
    }
}
