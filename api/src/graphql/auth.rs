//! GraphQL field guards.
//!
//! [`AuthRequirement`] is matched exhaustively over every [`AuthInfo`] variant, with
//! no `_ =>` catch-all, in every arm below. That's deliberate: adding a third
//! principal later will force every arm here to be revisited rather than silently
//! falling through to whatever the wildcard happened to do.

use async_graphql::Context;
use async_graphql::ErrorExtensions;
use async_graphql::Guard;

use crate::auth::{AuthInfo, Membership};

/// Build a guard-check failure carrying `extensions.code = "UNAUTHENTICATED"`.
/// Guard failures bypass the resolver entirely — there is no `anyhow::Result` for
/// an `ApiError` to ride through — so the code is set here directly rather than via
/// the central classification in `graphql::mod`'s extension, which only fills in a
/// code when one isn't already present.
fn unauthenticated(message: &str) -> async_graphql::Error {
    async_graphql::Error::new(message).extend_with(|_, e| e.set("code", "UNAUTHENTICATED"))
}

/// `pub(crate)`, not private: `query`/`mutations` need the same "is this
/// caller a member of this instance" check for ticket resolvers, whose
/// authorization has to happen inside the resolver body (per-record, after
/// fetching the ticket) rather than in a static `#[graphql(guard)]` — see
/// `graphql::mutations`' `require_ticket_member` doc comment.
pub(crate) fn is_member(memberships: &[Membership], instance_id: &str) -> bool {
    memberships.iter().any(|m| m.instance_id == instance_id)
}

pub(crate) fn is_owner(memberships: &[Membership], instance_id: &str) -> bool {
    memberships
        .iter()
        .any(|m| m.instance_id == instance_id && m.is_owner)
}

pub enum AuthRequirement {
    /// Any authenticated principal: a `User`, or a `Requester` holding a submit
    /// capability token.
    Authenticated,
    /// A `User` who belongs (in any role) to this instance.
    Member(String),
    /// A `User` who is an owner of this instance.
    InstanceOwner(String),
    /// The public submit form's capability token — never a `User`, however
    /// privileged.
    Requester,
}

pub struct AuthGuard {
    requirement: AuthRequirement,
}

impl AuthGuard {
    pub fn new(requirement: AuthRequirement) -> Self {
        Self { requirement }
    }
}

impl Guard for AuthGuard {
    async fn check(&self, ctx: &Context<'_>) -> async_graphql::Result<()> {
        let auth = ctx.data_opt::<AuthInfo>();
        match &self.requirement {
            AuthRequirement::Authenticated => {
                if match auth {
                    Some(AuthInfo::User { .. }) => true,
                    Some(AuthInfo::Requester { .. }) => true,
                    None => false,
                } {
                    Ok(())
                } else {
                    Err(unauthenticated("Must be authenticated"))
                }
            }
            AuthRequirement::Member(instance_id) => {
                if match auth {
                    Some(AuthInfo::User { memberships, .. }) => is_member(memberships, instance_id),
                    Some(AuthInfo::Requester { .. }) => false,
                    None => false,
                } {
                    Ok(())
                } else {
                    Err(unauthenticated("Must be a member of this instance"))
                }
            }
            AuthRequirement::InstanceOwner(instance_id) => {
                if match auth {
                    Some(AuthInfo::User { memberships, .. }) => is_owner(memberships, instance_id),
                    Some(AuthInfo::Requester { .. }) => false,
                    None => false,
                } {
                    Ok(())
                } else {
                    Err(unauthenticated("Must be an owner of this instance"))
                }
            }
            AuthRequirement::Requester => {
                if match auth {
                    Some(AuthInfo::Requester { .. }) => true,
                    Some(AuthInfo::User { .. }) => false,
                    None => false,
                } {
                    Ok(())
                } else {
                    Err(unauthenticated("Must provide a requester submit token"))
                }
            }
        }
    }
}
