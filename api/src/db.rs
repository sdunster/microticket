//! The database abstraction: [`Handler`] trait, [`Error`], and the row/paging types
//! every backend and every later domain module builds on.
//!
//! Step 3 (auth) adds `user`, `login_code`, `user_token`, `webauthn_credential`
//! and `ephemeral_state` below; step 4 adds `instance`/`membership`; step 5 adds
//! `ticket`/`ticket_message`/`counter`. Each of those steps grows [`Handler`]
//! with the methods it needs, following the shape of
//! [`crate::dynamodb::Handler`]'s generic infrastructure and
//! [`crate::mockdb::Handler`]'s all-fail mock.

use std::future::Future;
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

/// A `user` row — the only durable identity microticket has. Membership (which
/// instances they belong to, in what role) is step 4's table; nothing here names
/// an instance.
#[derive(Clone, Debug, PartialEq)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: u64,
    /// Absent until the user's first authenticated request. Touched at most once
    /// per minute (see `auth::fetch_update_user_auth_info`) to bound write volume.
    pub access_time: Option<u64>,
}

impl HasID for User {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Update shapes for `user`, seslogin's convention (an enum of named shapes
/// rather than a struct of `Option<T>` fields) — each variant is exactly the
/// attributes one call site needs to touch, so a caller cannot accidentally
/// clobber a field it never meant to change.
#[derive(Clone, Debug, PartialEq)]
pub enum UserUpdateShape<'a> {
    Fields {
        name: &'a str,
        enabled: bool,
    },
    /// Throttled touch of `access_time` on a successful authenticated request;
    /// see [`crate::auth`].
    AccessTime,
}

/// A pending email login code. Hash key is `email` itself (see `SCHEMA.md`) — at
/// most one outstanding code per address, which is also what makes the 30s
/// resend rate limit a single-row read.
#[derive(Clone, Debug, PartialEq)]
pub struct LoginCode {
    pub email: String,
    pub code_hash: String,
    pub expires_at: u64,
    pub attempts: u64,
    pub last_sent_at: u64,
}

/// An opaque `mtu_` session token. Only `token_hash` (sha256 of the secret) is
/// ever stored — the secret itself exists only at issuance, as the string
/// returned by `auth::issue_user_token`.
#[derive(Clone, Debug, PartialEq)]
pub struct UserToken {
    pub id: String,
    pub token_hash: String,
    pub user_id: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub last_used_at: Option<u64>,
}

impl HasID for UserToken {
    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserTokenUpdateShape {
    TouchLastUsed,
}

/// A registered WebAuthn/passkey credential. `passkey_json` is the serialized
/// `webauthn_rs::prelude::Passkey` — opaque to everything except the
/// `webauthn-rs` crate, and the thing the serialized-fixture regression test in
/// `graphql::mutations` exists to protect.
#[derive(Clone, Debug, PartialEq)]
pub struct WebauthnCredential {
    /// Credential ID (base64url), also the DynamoDB hash key.
    pub id: String,
    pub user_id: String,
    /// User-supplied label, shown in the settings page.
    pub name: String,
    pub passkey_json: String,
    pub created_at: u64,
    pub last_used_at: Option<u64>,
}

impl HasID for WebauthnCredential {
    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebauthnCredentialUpdate {
    Rename(String),
    /// Always written on a successful login, even when the signature counter
    /// itself did not advance — see the doc comment at the `finish_passkey_login`
    /// call site for why gating this on `needs_update()` would leave
    /// `last_used_at` perpetually unset for most synced passkeys.
    TouchLastUsed {
        passkey_json: String,
    },
}

/// A row in the generic, TTL'd `ephemeral_state` table: a `kind`-namespaced
/// key/value capability store. Backs WebAuthn challenge state (`kind` "reg" /
/// "auth") in this step, and will back the requester submit-token flow's
/// capability token (`kind` "submit_token") in step 4 — the whole reason this is
/// written generically rather than as a WebAuthn-specific table. `payload` is
/// opaque JSON whose shape depends on `kind`.
#[derive(Clone, Debug, PartialEq)]
pub struct EphemeralState {
    pub id: String,
    pub kind: String,
    pub payload: String,
    pub expires_at: u64,
}

/// A WebAuthn registration/login challenge, as a typed view over an
/// `ephemeral_state` row (`kind` "reg" or "auth"). Not a separate table — see
/// [`Handler::put_webauthn_state`]'s doc comment for how `user_id` and
/// `state_json` fold into that row's opaque `payload`.
#[derive(Clone, Debug, PartialEq)]
pub struct WebauthnState {
    pub id: String,
    pub kind: String,
    /// Set for registration challenges (so `finishPasskeyRegistration` can check
    /// the challenge belongs to the caller); absent for login challenges, which
    /// are usernameless by design.
    pub user_id: Option<String>,
    /// JSON-serialized `PasskeyRegistration` or `DiscoverableAuthentication`.
    pub state_json: String,
    pub expires_at: u64,
}

/// `Sync` is required so a `&impl Handler` (including the erased handle returned by
/// [`crate::app::HasDb::db`]) can be held across `.await` inside the `Send` futures
/// the GraphQL/Poem stack builds. Both implementors ([`crate::dynamodb::Handler`],
/// [`crate::mockdb::Handler`]) are already `Sync`.
///
/// Every method here follows the RPITIT style already used by
/// [`crate::mail::Handler`]: `fn foo(&self, ...) -> impl Future<Output = Result<T>> + Send`,
/// not `async_trait`.
pub trait Handler: Sync {
    // ── user ──────────────────────────────────────────────────────────────
    fn get_users<T: AsRef<str> + Sync>(
        &self,
        ids: &[T],
    ) -> impl Future<Output = Result<Vec<Option<User>>>> + Send;
    /// Resolve an email to its user id via `email-index`, collapsed through
    /// [`at_most_one`] — unlike seslogin's raw `Vec<String>`, callers here get a
    /// single answer directly, since every call site immediately wants "the one
    /// user with this email, if any" rather than the raw index hits.
    fn get_user_id_by_email(
        &self,
        email: &str,
    ) -> impl Future<Output = Result<Option<String>>> + Send;
    fn create_user(&self, email: &str, name: &str) -> impl Future<Output = Result<User>> + Send;
    fn update_user(
        &self,
        id: &str,
        change: UserUpdateShape<'_>,
    ) -> impl Future<Output = Result<()>> + Send;

    // ── login_code ────────────────────────────────────────────────────────
    fn put_login_code(
        &self,
        email: &str,
        code_hash: &str,
        expires_at: u64,
        now: u64,
    ) -> impl Future<Output = Result<()>> + Send;
    fn get_login_code(&self, email: &str)
    -> impl Future<Output = Result<Option<LoginCode>>> + Send;
    fn delete_login_code(&self, email: &str) -> impl Future<Output = Result<()>> + Send;
    fn increment_login_code_attempts(&self, email: &str)
    -> impl Future<Output = Result<()>> + Send;

    // ── user_token ────────────────────────────────────────────────────────
    fn create_user_token(
        &self,
        token_hash: &str,
        user_id: &str,
        expires_at: u64,
    ) -> impl Future<Output = Result<UserToken>> + Send;
    fn get_user_token_by_hash(
        &self,
        token_hash: &str,
    ) -> impl Future<Output = Result<Option<UserToken>>> + Send;
    fn update_user_token(
        &self,
        id: &str,
        change: UserTokenUpdateShape,
    ) -> impl Future<Output = Result<()>> + Send;
    fn delete_user_token(&self, id: &str) -> impl Future<Output = Result<()>> + Send;

    // ── webauthn_credential ───────────────────────────────────────────────
    fn create_webauthn_credential(
        &self,
        id: &str,
        user_id: &str,
        name: &str,
        passkey_json: &str,
    ) -> impl Future<Output = Result<WebauthnCredential>> + Send;
    fn get_webauthn_credential(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<WebauthnCredential>>> + Send;
    fn list_webauthn_credentials_by_user(
        &self,
        user_id: &str,
    ) -> impl Future<Output = Result<Vec<WebauthnCredential>>> + Send;
    fn count_webauthn_credentials_by_user(
        &self,
        user_id: &str,
    ) -> impl Future<Output = Result<usize>> + Send;
    fn update_webauthn_credential(
        &self,
        id: &str,
        change: WebauthnCredentialUpdate,
    ) -> impl Future<Output = Result<()>> + Send;
    fn delete_webauthn_credential(&self, id: &str) -> impl Future<Output = Result<()>> + Send;

    // ── ephemeral_state (generic) ────────────────────────────────────────────
    /// Upsert a record into the `ephemeral_state` table (overwrites any existing
    /// item with the same `id`).
    fn put_ephemeral_state(
        &self,
        id: &str,
        kind: &str,
        payload: &str,
        expires_at: u64,
    ) -> impl Future<Output = Result<()>> + Send;
    fn get_ephemeral_state(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<EphemeralState>>> + Send;
    fn delete_ephemeral_state(&self, id: &str) -> impl Future<Output = Result<()>> + Send;

    // ── WebAuthn challenge state (a typed view over ephemeral_state) ─────────
    /// `kind` is `"reg"` or `"auth"`; `user_id` and `state_json` are folded into
    /// the `ephemeral_state` row's opaque JSON `payload` (see
    /// [`WebauthnState`]'s doc comment) rather than becoming attributes of their
    /// own — the table stays generic so step 4's requester submit token can reuse
    /// it without the schema accreting WebAuthn-specific columns.
    fn put_webauthn_state(
        &self,
        id: &str,
        kind: &str,
        user_id: Option<&str>,
        state_json: &str,
        expires_at: u64,
    ) -> impl Future<Output = Result<()>> + Send;
    fn get_webauthn_state(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<WebauthnState>>> + Send;
    fn delete_webauthn_state(&self, id: &str) -> impl Future<Output = Result<()>> + Send;
}

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
