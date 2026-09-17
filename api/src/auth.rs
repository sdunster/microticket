//! Authentication principals and the `Authorization` header dispatcher.
//!
//! Ported from seslogin's `auth.rs`, minus the kiosk/session and API-token
//! principals microticket has no equivalent of. There are exactly two principals:
//! an authenticated [`AuthInfo::User`] (a member, possibly of several instances)
//! and an anonymous [`AuthInfo::Requester`] holding a short-lived, single-purpose
//! capability token scoped to one instance (the public submit form's flow — the
//! same pattern as seslogin's `period_link.rs`). Only `User` is real as of this
//! step; `Requester` becomes reachable in step 4, once there is an instance id to
//! scope it to.

use thiserror::Error;
use tracing::warn;

use crate::app::{App, HasDb};
use crate::db;
use crate::db::Handler as _;

#[derive(Debug, Error)]
pub enum AuthError {
    /// Token is definitively invalid — bad token, expired, record not found, etc.
    /// Surfaced as 401.
    #[error("{0}")]
    Permanent(String),
    /// Infrastructure failure during verification — DB down, network error, etc.
    /// Surfaced as 503, since retrying may succeed.
    #[error("{0}")]
    Transient(String),
}

/// Classify a `db::Error` encountered while verifying a token: a definitive
/// "doesn't exist" is a bad credential (401); anything else might succeed on
/// retry (503), so it must not be treated the same as an invalid token.
fn classify_db_err(msg: &str, e: db::Error) -> AuthError {
    match e {
        db::Error::NotFound(_) => AuthError::Permanent(format!("{msg}: {e:#}")),
        _ => AuthError::Transient(format!("{msg}: {e:#}")),
    }
}

/// One membership of an [`AuthInfo::User`] in an instance.
///
/// A thin, auth-time-only shape — not the `membership` table's row type, which
/// step 4 defines in `db.rs` alongside the rest of the instance/membership domain
/// and which this will eventually be built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub instance_id: String,
    pub is_owner: bool,
}

pub enum AuthInfo {
    /// An authenticated member, holding every instance they belong to (so a guard
    /// can check `Member(instance)`/`InstanceOwner(instance)` without a DB round
    /// trip per field).
    User {
        id: String,
        /// Always empty until step 4 adds the `membership` table and a real
        /// lookup (`get_memberships_by_user` or similar) here. Left as a real
        /// field now — rather than invented later — so every guard and resolver
        /// that reads it is already written against its final shape; an empty
        /// vec just means `Member`/`InstanceOwner` guards correctly reject
        /// everyone, since there is nothing to be a member of yet.
        memberships: Vec<Membership>,
        /// Set only when authenticated via an opaque `mtu_` token (always true
        /// today — there is no other way to authenticate as a `User` yet), so
        /// `logout` can revoke exactly the token that was presented.
        token_id: Option<String>,
    },
    /// The public submit form's short-lived capability token: authorises exactly
    /// `submitTicket` for one `(email, instance_id)` pair, and nothing else.
    /// Unreachable until step 4.
    Requester { email: String, instance_id: String },
}

/// Prefix of an opaque user session token, sha256-hashed and stored in
/// `user_token`. No JWTs in this project — every credential is an opaque secret
/// whose hash is looked up in DynamoDB, per `CLAUDE.md`/the build plan.
pub const USER_TOKEN_PREFIX: &str = "mtu_";

/// Dev-only auth override configured via a CLI flag on the poem server. When set,
/// the server bypasses token verification entirely and treats every request as
/// the configured caller. Intended for local UI testing/screenshots only — never
/// enable in a deployed environment. Absent from the Lambda binary: `bin/lambda`
/// never constructs a `DevAuthConfig` in the first place, since it has no CLI to
/// read a flag from.
///
/// Only a `User` variant exists (unlike seslogin's session/user split) — there is
/// no kiosk-equivalent principal to impersonate.
pub enum DevAuthConfig {
    User { id_or_email: String },
}

/// Resolve a [`DevAuthConfig`] into an [`AuthInfo`] without any token check.
///
/// Impersonation keeps the impersonated caller's *real* permissions — it resolves
/// the same [`AuthInfo::User`] a normal login would produce (memberships
/// included, once step 4 populates them), rather than granting some synthetic
/// elevated principal. `token_id` is `None`: there is no real token to revoke, so
/// `logout` is a no-op for an impersonated session.
pub async fn resolve_dev_auth<A: App + HasDb>(
    app: &A,
    config: &DevAuthConfig,
) -> Result<AuthInfo, AuthError> {
    match config {
        DevAuthConfig::User { id_or_email } => {
            let user_id = if id_or_email.contains('@') {
                app.db()
                    .get_user_id_by_email(id_or_email)
                    .await
                    .map_err(|e| classify_db_err("dev auth: fetch user by email", e))?
                    .ok_or_else(|| {
                        AuthError::Permanent(format!("Dev auth user not found: {id_or_email}"))
                    })?
            } else {
                id_or_email.clone()
            };
            fetch_update_user_auth_info(app, user_id).await
        }
    }
}

/// What kind of caller made a request, used as a telemetry/logging dimension.
///
/// The string forms are a stable log contract: CloudWatch Logs Insights queries and
/// metric filters match on them, so renaming a variant's string changes what those
/// queries return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CallerType {
    User,
    Requester,
    #[default]
    Unauthenticated,
}

impl CallerType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Requester => "requester",
            Self::Unauthenticated => "unauthenticated",
        }
    }
}

impl std::fmt::Display for CallerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Maps an optional [`AuthInfo`] to `(caller_type, caller_id)` for telemetry/logging.
pub fn caller_info(auth: Option<&AuthInfo>) -> (CallerType, String) {
    match auth {
        None => (CallerType::Unauthenticated, "unknown".to_owned()),
        Some(AuthInfo::User { id, .. }) => (CallerType::User, id.clone()),
        Some(AuthInfo::Requester { email, .. }) => (CallerType::Requester, email.clone()),
    }
}

/// Hex SHA-256 of a token secret. The stored row is keyed by this, never the raw
/// token — see the house rule against storing secrets in `CLAUDE.md`'s spirit
/// (not written down there specifically, but the same principle: a DB/PITR leak
/// must not expose a usable credential).
fn hash_token(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hex::encode(hasher.finalize())
}

/// Issue a fresh opaque `mtu_` token for `user_id`. Only its sha256 hash is
/// stored; the secret returned here is the only time it ever exists in full —
/// callers (`verifyAuthCode`, `finishPasskeyLogin`) hand it straight back to the
/// client and keep nothing.
pub async fn issue_user_token<A: App + HasDb>(app: &A, user_id: &str) -> anyhow::Result<String> {
    let secret = format!("{}{}", USER_TOKEN_PREFIX, crate::nonce::generate_nonce(32));
    let hash = hash_token(&secret);
    let expires_at = crate::expire::ExpirePolicy::UserToken.from_now();
    app.db()
        .create_user_token(&hash, user_id, expires_at)
        .await?;
    Ok(secret)
}

/// Fetch a user's DB record, reject if disabled, and throttle-touch
/// `access_time`. Shared by every path that resolves to an [`AuthInfo::User`]
/// (opaque-token verification and `--dev-auth-user`), so "what does it take for
/// a user id to become a valid principal" is defined in exactly one place.
/// Always returns `token_id: None`; callers that authenticated via a token fill
/// it in themselves afterward.
async fn fetch_update_user_auth_info<A: App + HasDb>(
    app: &A,
    user_id: String,
) -> Result<AuthInfo, AuthError> {
    let users = app
        .db()
        .get_users(&[&user_id])
        .await
        .map_err(|e| classify_db_err("fetch user from db", e))?;
    let user = users
        .into_iter()
        .next()
        .flatten()
        .ok_or_else(|| AuthError::Permanent("User not found".into()))?;

    if !user.enabled {
        return Err(AuthError::Permanent("User account is disabled".into()));
    }

    // Throttled: only write access_time if it's stale by more than a minute, to
    // reduce DB write load on the hot path of every authenticated request.
    let now = crate::clock::now_sec();
    if user.access_time.is_none_or(|t| now > t + 60) {
        match app
            .db()
            .update_user(&user_id, db::UserUpdateShape::AccessTime)
            .await
        {
            Ok(_) => {}
            Err(db::Error::MutationDisabled) => {
                warn!("update_user skipped: mutations disabled");
            }
            Err(e) => return Err(AuthError::Transient(e.to_string())),
        }
    }

    // memberships: empty until step 4's membership table lands a real lookup
    // here (see the field doc on `AuthInfo::User`).
    Ok(AuthInfo::User {
        id: user_id,
        memberships: Vec::new(),
        token_id: None,
    })
}

async fn verify_token_with_user_token<A: App + HasDb>(
    app: &A,
    token: &str,
) -> Result<AuthInfo, AuthError> {
    let token_hash = hash_token(token);
    let user_token = app
        .db()
        .get_user_token_by_hash(&token_hash)
        .await
        .map_err(|e| classify_db_err("fetch user token by hash", e))?
        .ok_or_else(|| AuthError::Permanent("Invalid user token".into()))?;

    let now = crate::clock::now_sec();
    if now >= user_token.expires_at {
        return Err(AuthError::Permanent("User token has expired".into()));
    }

    let token_id = user_token.id.clone();

    // Throttled touch, same 60s window as access_time above.
    if user_token.last_used_at.is_none_or(|t| now > t + 60) {
        match app
            .db()
            .update_user_token(&user_token.id, db::UserTokenUpdateShape::TouchLastUsed)
            .await
        {
            Ok(_) => {}
            Err(db::Error::MutationDisabled) => {
                warn!("update_user_token skipped: mutations disabled");
            }
            Err(e) => return Err(AuthError::Transient(e.to_string())),
        }
    }

    match fetch_update_user_auth_info(app, user_token.user_id).await? {
        AuthInfo::User {
            id, memberships, ..
        } => Ok(AuthInfo::User {
            id,
            memberships,
            token_id: Some(token_id),
        }),
        other => Ok(other),
    }
}

/// Dispatch an opaque token to the right verifier by its prefix. `mtu_` is the
/// only recognized scheme in this project (no JWTs) — anything else is a
/// definitively bad credential, not a "try the next scheme" fallthrough.
pub async fn verify_token<A: App + HasDb>(app: &A, token: &str) -> Result<AuthInfo, AuthError> {
    if token.starts_with(USER_TOKEN_PREFIX) {
        return verify_token_with_user_token(app, token).await;
    }
    Err(AuthError::Permanent("Unrecognized token".into()))
}

/// Dispatch an `Authorization` header value: `Bearer <token>` is the only scheme
/// microticket has (no cookies, no signed-kiosk-key scheme like seslogin's `SLKey`).
/// Returns `None` when there is no recognized header, so the request proceeds
/// unauthenticated and the GraphQL guards (`AuthRequirement`) reject anything that
/// requires a principal. A `Bearer` header that *is* present but doesn't verify
/// always yields `Some(Err(..))` — a bad credential is not silently treated as no
/// credential.
pub async fn verify_authorization_header<A: App + HasDb>(
    app: &A,
    auth_header: Option<&str>,
) -> Option<Result<AuthInfo, AuthError>> {
    let auth_header = auth_header?;
    let token = auth_header.strip_prefix("Bearer ")?;
    Some(verify_token(app, token).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_info_for_no_auth() {
        let (kind, id) = caller_info(None);
        assert_eq!(kind, CallerType::Unauthenticated);
        assert_eq!(id, "unknown");
    }

    #[test]
    fn caller_info_for_user() {
        let auth = AuthInfo::User {
            id: "u1".into(),
            memberships: vec![],
            token_id: Some("tok1".into()),
        };
        let (kind, id) = caller_info(Some(&auth));
        assert_eq!(kind, CallerType::User);
        assert_eq!(id, "u1");
    }

    #[test]
    fn caller_info_for_requester() {
        let auth = AuthInfo::Requester {
            email: "r@example.com".into(),
            instance_id: "inst1".into(),
        };
        let (kind, id) = caller_info(Some(&auth));
        assert_eq!(kind, CallerType::Requester);
        assert_eq!(id, "r@example.com");
    }

    #[test]
    fn caller_type_strings_are_stable() {
        assert_eq!(CallerType::User.as_str(), "user");
        assert_eq!(CallerType::Requester.as_str(), "requester");
        assert_eq!(CallerType::Unauthenticated.as_str(), "unauthenticated");
    }

    #[test]
    fn hash_token_is_stable_hex_sha256() {
        let h = hash_token("mtu_abc");
        assert_eq!(h, hash_token("mtu_abc"));
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(h, hash_token("mtu_xyz"));
    }

    #[test]
    fn classify_db_err_maps_not_found_to_permanent() {
        let e = classify_db_err("x", db::Error::NotFound("missing".into()));
        assert!(matches!(e, AuthError::Permanent(_)));
    }

    #[test]
    fn classify_db_err_maps_everything_else_to_transient() {
        for e in [
            db::Error::Infrastructure("boom".into()),
            db::Error::Hydration("boom".into()),
            db::Error::Integrity("boom".into()),
            db::Error::TypeConversion("boom".into()),
            db::Error::MutationDisabled,
        ] {
            assert!(matches!(classify_db_err("x", e), AuthError::Transient(_)));
        }
    }

    #[tokio::test]
    async fn verify_token_rejects_an_unrecognized_prefix() {
        use crate::app;
        use crate::mockdb;
        use crate::mockmail;

        let my_app = app::new(mockdb::Handler::new(), mockmail::Handler::new(), 0);
        let result = verify_token(&my_app, "slu_not_our_scheme").await;
        assert!(matches!(result, Err(AuthError::Permanent(_))));
    }

    #[tokio::test]
    async fn verify_authorization_header_ignores_a_missing_header() {
        use crate::app;
        use crate::mockdb;
        use crate::mockmail;

        let my_app = app::new(mockdb::Handler::new(), mockmail::Handler::new(), 0);
        assert!(verify_authorization_header(&my_app, None).await.is_none());
    }

    #[tokio::test]
    async fn verify_authorization_header_ignores_a_non_bearer_scheme() {
        use crate::app;
        use crate::mockdb;
        use crate::mockmail;

        let my_app = app::new(mockdb::Handler::new(), mockmail::Handler::new(), 0);
        assert!(
            verify_authorization_header(&my_app, Some("Basic dXNlcjpwYXNz"))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn verify_authorization_header_surfaces_a_bad_bearer_token_as_an_error() {
        use crate::app;
        use crate::mockdb;
        use crate::mockmail;

        let my_app = app::new(mockdb::Handler::new(), mockmail::Handler::new(), 0);
        let result = verify_authorization_header(&my_app, Some("Bearer garbage"))
            .await
            .expect("a Bearer header must always be checked, never silently ignored");
        assert!(result.is_err());
    }
}
