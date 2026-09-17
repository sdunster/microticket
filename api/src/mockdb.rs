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

    /// Every method routes through this rather than inventing its own error, so a
    /// mockdb failure always reads the same way regardless of which method
    /// produced it.
    fn unsupported<T>() -> db::Result<T> {
        Err(db::Error::Infrastructure(
            "mockdb operation not implemented".to_string(),
        ))
    }
}

impl db::Handler for Handler {
    // ── user ──────────────────────────────────────────────────────────────

    async fn get_users<T: AsRef<str> + Sync>(
        &self,
        _ids: &[T],
    ) -> db::Result<Vec<Option<db::User>>> {
        Self::unsupported()
    }

    async fn get_user_id_by_email(&self, _email: &str) -> db::Result<Option<String>> {
        Self::unsupported()
    }

    async fn create_user(&self, _email: &str, _name: &str) -> db::Result<db::User> {
        Self::unsupported()
    }

    async fn update_user(&self, _id: &str, _change: db::UserUpdateShape<'_>) -> db::Result<()> {
        Self::unsupported()
    }

    // ── login_code ────────────────────────────────────────────────────────

    async fn put_login_code(
        &self,
        _email: &str,
        _code_hash: &str,
        _expires_at: u64,
        _now: u64,
    ) -> db::Result<()> {
        Self::unsupported()
    }

    async fn get_login_code(&self, _email: &str) -> db::Result<Option<db::LoginCode>> {
        Self::unsupported()
    }

    async fn delete_login_code(&self, _email: &str) -> db::Result<()> {
        Self::unsupported()
    }

    async fn increment_login_code_attempts(&self, _email: &str) -> db::Result<()> {
        Self::unsupported()
    }

    // ── user_token ────────────────────────────────────────────────────────

    async fn create_user_token(
        &self,
        _token_hash: &str,
        _user_id: &str,
        _expires_at: u64,
    ) -> db::Result<db::UserToken> {
        Self::unsupported()
    }

    async fn get_user_token_by_hash(&self, _token_hash: &str) -> db::Result<Option<db::UserToken>> {
        Self::unsupported()
    }

    async fn update_user_token(
        &self,
        _id: &str,
        _change: db::UserTokenUpdateShape,
    ) -> db::Result<()> {
        Self::unsupported()
    }

    async fn delete_user_token(&self, _id: &str) -> db::Result<()> {
        Self::unsupported()
    }

    // ── webauthn_credential ───────────────────────────────────────────────

    async fn create_webauthn_credential(
        &self,
        _id: &str,
        _user_id: &str,
        _name: &str,
        _passkey_json: &str,
    ) -> db::Result<db::WebauthnCredential> {
        Self::unsupported()
    }

    async fn get_webauthn_credential(
        &self,
        _id: &str,
    ) -> db::Result<Option<db::WebauthnCredential>> {
        Self::unsupported()
    }

    async fn list_webauthn_credentials_by_user(
        &self,
        _user_id: &str,
    ) -> db::Result<Vec<db::WebauthnCredential>> {
        Self::unsupported()
    }

    async fn count_webauthn_credentials_by_user(&self, _user_id: &str) -> db::Result<usize> {
        Self::unsupported()
    }

    async fn update_webauthn_credential(
        &self,
        _id: &str,
        _change: db::WebauthnCredentialUpdate,
    ) -> db::Result<()> {
        Self::unsupported()
    }

    async fn delete_webauthn_credential(&self, _id: &str) -> db::Result<()> {
        Self::unsupported()
    }

    // ── ephemeral_state (generic) ────────────────────────────────────────────

    async fn put_ephemeral_state(
        &self,
        _id: &str,
        _kind: &str,
        _payload: &str,
        _expires_at: u64,
    ) -> db::Result<()> {
        Self::unsupported()
    }

    async fn get_ephemeral_state(&self, _id: &str) -> db::Result<Option<db::EphemeralState>> {
        Self::unsupported()
    }

    async fn delete_ephemeral_state(&self, _id: &str) -> db::Result<()> {
        Self::unsupported()
    }

    // ── WebAuthn challenge state ──────────────────────────────────────────────

    async fn put_webauthn_state(
        &self,
        _id: &str,
        _kind: &str,
        _user_id: Option<&str>,
        _state_json: &str,
        _expires_at: u64,
    ) -> db::Result<()> {
        Self::unsupported()
    }

    async fn get_webauthn_state(&self, _id: &str) -> db::Result<Option<db::WebauthnState>> {
        Self::unsupported()
    }

    async fn delete_webauthn_state(&self, _id: &str) -> db::Result<()> {
        Self::unsupported()
    }
}
