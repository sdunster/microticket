//! `QueryRoot` and the `User` GraphQL object.
//!
//! Split into its own module the way seslogin splits `query.rs` from
//! `mutations.rs` — `graphql::mod` just wires the two together into a schema.

use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_graphql::{Context, ID, Object, SimpleObject};

use crate::app::{App, HasDb};
use crate::auth::AuthInfo;
use crate::db;
use crate::db::Handler as _;

use super::auth::{AuthGuard, AuthRequirement};

/// Metadata for a stored passkey credential — never the credential itself (no
/// private key material, no raw `passkey_json`).
#[derive(SimpleObject, Clone, Debug)]
pub struct PasskeyInfo {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

impl From<db::WebauthnCredential> for PasskeyInfo {
    fn from(c: db::WebauthnCredential) -> Self {
        Self {
            id: c.id,
            name: c.name,
            created_at: c.created_at as i64,
            last_used_at: c.last_used_at.map(|t| t as i64),
        }
    }
}

/// A `user` row, exposed over GraphQL. Generic over `A` (rather than holding a
/// concrete app type) so it composes with `build_schema`'s per-binary `A`, the
/// same reason `MutationRoot<A>` is generic — see `graphql::mod`'s doc comment.
#[derive(Debug, PartialEq)]
pub struct User<A: App + HasDb + Send + Sync> {
    _marker: PhantomData<A>,
    rec: db::User,
}

impl<A: App + HasDb + Send + Sync> User<A> {
    pub fn new(rec: db::User) -> Self {
        Self {
            _marker: PhantomData,
            rec,
        }
    }
}

impl<A: App + HasDb + Send + Sync> Clone for User<A> {
    fn clone(&self) -> Self {
        Self {
            _marker: PhantomData,
            rec: self.rec.clone(),
        }
    }
}

#[Object]
impl<A: App + HasDb + Send + Sync + 'static> User<A> {
    async fn id(&self) -> ID {
        ID(self.rec.id.clone())
    }
    async fn email(&self) -> &str {
        &self.rec.email
    }
    async fn name(&self) -> &str {
        &self.rec.name
    }
    async fn enabled(&self) -> bool {
        self.rec.enabled
    }
    async fn created_at(&self) -> i64 {
        self.rec.created_at as i64
    }
    async fn access_time(&self) -> Option<i64> {
        self.rec.access_time.map(|t| t as i64)
    }

    /// The caller's own registered passkeys. Guarded again even though the only
    /// current path to a `User` object (`me`) is already guarded — defence in
    /// depth against a future unguarded resolver that returns a `User` (e.g. an
    /// admin-facing user lookup) accidentally leaking another user's passkey
    /// metadata.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn passkeys(&self, ctx: &Context<'_>) -> Result<Vec<PasskeyInfo>> {
        let app = ctx.data_unchecked::<Arc<A>>();
        let creds = app
            .db()
            .list_webauthn_credentials_by_user(&self.rec.id)
            .await?;
        Ok(creds.into_iter().map(PasskeyInfo::from).collect())
    }
}

pub struct QueryRoot<A: App + HasDb + Send + Sync> {
    _marker: PhantomData<A>,
}

impl<A: App + HasDb + Send + Sync> QueryRoot<A> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<A: App + HasDb + Send + Sync> Default for QueryRoot<A> {
    fn default() -> Self {
        Self::new()
    }
}

#[Object]
impl<A: App + HasDb + Send + Sync + 'static> QueryRoot<A> {
    /// API build version — the git commit this server was built from.
    async fn version(&self) -> String {
        crate::environment::GIT_REV.to_string()
    }

    /// The authenticated caller's own user record. Guarded on `Authenticated`
    /// (the only requirement microticket's guard enum offers that fits a "must
    /// have *some* credential" query), but only a `User` principal actually has
    /// a record to return — a `Requester` capability token hits the explicit
    /// error below rather than some confusing "not found".
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn me(&self, ctx: &Context<'_>) -> Result<User<A>> {
        let Some(AuthInfo::User { id, .. }) = ctx.data_opt::<AuthInfo>() else {
            return Err(anyhow!("Only an authenticated user has a `me` record"));
        };
        let app = ctx.data_unchecked::<Arc<A>>();
        let rec = app
            .db()
            .get_users(&[id])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| anyhow!("User with ID {id} missing"))?;
        Ok(User::new(rec))
    }
}
