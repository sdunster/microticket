//! Authentication principals and the `Authorization` header dispatcher.
//!
//! Ported from seslogin's `auth.rs`, minus the kiosk/session and API-token
//! principals microticket has no equivalent of. There are exactly two principals:
//! an authenticated [`AuthInfo::User`] (a member, possibly of several instances)
//! and an anonymous [`AuthInfo::Requester`] holding a short-lived, single-purpose
//! capability token scoped to one instance (the public submit form's flow — the
//! same pattern as seslogin's `period_link.rs`).
//!
//! Verifying an actual token is step 3's job — see the `TODO` in
//! [`verify_authorization_header`]. This module exists now so the GraphQL guards
//! (`graphql::auth`) and every later resolver have a stable `AuthInfo` shape to
//! code against, and so the dev server's `--dev-auth-user` flag has somewhere to
//! plug in once it works.

use thiserror::Error;

use crate::app::{App, HasDb};

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
        memberships: Vec<Membership>,
    },
    /// The public submit form's short-lived capability token: authorises exactly
    /// `submitTicket` for one `(email, instance_id)` pair, and nothing else.
    Requester { email: String, instance_id: String },
}

/// Dev-only auth override configured via a CLI flag on the poem server. When set,
/// the server is meant to bypass token verification entirely and treat every
/// request as the configured caller. Intended for local UI testing/screenshots
/// only — never enable in a deployed environment.
///
/// Only a `User` variant exists (unlike seslogin's session/user split) — there is
/// no kiosk-equivalent principal to impersonate.
pub enum DevAuthConfig {
    User { id_or_email: String },
}

/// Resolve a [`DevAuthConfig`] into an [`AuthInfo`] without any token check.
///
/// TODO(step 3/4): once `user` and `membership` exist, look the user up by id or
/// email (as seslogin's `resolve_dev_auth` does) and build their `AuthInfo::User`.
/// Until then this always fails loudly, so `--dev-auth-user` visibly doesn't work
/// yet rather than silently authenticating as nobody.
pub async fn resolve_dev_auth<A: App + HasDb>(
    _app: &A,
    _config: &DevAuthConfig,
) -> Result<AuthInfo, AuthError> {
    Err(AuthError::Permanent(
        "dev auth override: not implemented until the user/membership tables land".into(),
    ))
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

/// Prefix of an opaque user session token, sha256-hashed and stored in
/// `user_token` (step 3). Not yet checked anywhere — see
/// [`verify_authorization_header`].
pub const USER_TOKEN_PREFIX: &str = "mtu_";

/// Dispatch an `Authorization` header value: `Bearer <token>` is the only scheme
/// microticket has (no cookies, no signed-kiosk-key scheme like seslogin's `SLKey`).
/// Returns `None` when there is no recognized header, so the request proceeds
/// unauthenticated and the GraphQL guards (`AuthRequirement`) reject anything that
/// requires a principal.
///
/// TODO(step 3): actually verify the token — an `mtu_`-prefixed opaque token
/// against `user_token`, or a requester submit token against `ephemeral_state` —
/// and return the matching `AuthInfo`. Until then every `Bearer` token is treated
/// as unrecognized (`None`), not as an error: it's not that the token is wrong,
/// it's that nothing can check it yet.
pub async fn verify_authorization_header<A: App + HasDb>(
    _app: &A,
    auth_header: Option<&str>,
) -> Option<Result<AuthInfo, AuthError>> {
    let auth_header = auth_header?;
    let _token = auth_header.strip_prefix("Bearer ")?;
    None
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
}
