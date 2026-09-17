//! `QueryRoot` and the `User` GraphQL object.
//!
//! Split into its own module the way seslogin splits `query.rs` from
//! `mutations.rs` — `graphql::mod` just wires the two together into a schema.

use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_graphql::{Context, Enum, ID, Object, SimpleObject};

use crate::app::{App, HasDb};
use crate::auth::AuthInfo;
use crate::db;
use crate::db::Handler as _;

use super::auth::{AuthGuard, AuthRequirement};
use super::error::ApiError;

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

/// `membership.role`, exposed over GraphQL.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum MembershipRoleType {
    Owner,
    Agent,
}

impl From<db::MembershipRole> for MembershipRoleType {
    fn from(role: db::MembershipRole) -> Self {
        match role {
            db::MembershipRole::Owner => Self::Owner,
            db::MembershipRole::Agent => Self::Agent,
        }
    }
}

impl From<MembershipRoleType> for db::MembershipRole {
    fn from(role: MembershipRoleType) -> Self {
        match role {
            MembershipRoleType::Owner => Self::Owner,
            MembershipRoleType::Agent => Self::Agent,
        }
    }
}

/// `inbound_address.kind`, exposed over GraphQL.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum AddressKindType {
    Exact,
    Wildcard,
}

impl From<db::AddressKind> for AddressKindType {
    fn from(kind: db::AddressKind) -> Self {
        match kind {
            db::AddressKind::Exact => Self::Exact,
            db::AddressKind::Wildcard => Self::Wildcard,
        }
    }
}

/// An `inbound_address` row, exposed over GraphQL. Only reachable through
/// `Instance.inboundAddresses`, which is owner-guarded — see that field's doc
/// comment.
#[derive(SimpleObject, Clone, Debug)]
pub struct InboundAddressInfo {
    pub address: String,
    pub kind: AddressKindType,
    pub created_at: i64,
}

impl From<db::InboundAddress> for InboundAddressInfo {
    fn from(a: db::InboundAddress) -> Self {
        Self {
            address: a.address,
            kind: a.kind.into(),
            created_at: a.created_at as i64,
        }
    }
}

/// One instance, exposed publicly and unauthenticated via `publicInstances` —
/// **only** the name and slug, so nothing else about a tenant (its inbound
/// addresses, its members, whether it even has any) leaks to an anonymous
/// caller browsing `/submit`.
#[derive(SimpleObject, Clone, Debug)]
pub struct PublicInstance {
    pub name: String,
    pub slug: String,
}

impl From<db::Instance> for PublicInstance {
    fn from(i: db::Instance) -> Self {
        Self {
            name: i.name,
            slug: i.slug,
        }
    }
}

/// An `instance` row, exposed over GraphQL to its members. Generic over `A`
/// for the same reason as [`User`] — see that type's doc comment.
#[derive(Debug, PartialEq)]
pub struct Instance<A: App + HasDb + Send + Sync> {
    _marker: PhantomData<A>,
    rec: db::Instance,
}

impl<A: App + HasDb + Send + Sync> Instance<A> {
    pub fn new(rec: db::Instance) -> Self {
        Self {
            _marker: PhantomData,
            rec,
        }
    }
}

impl<A: App + HasDb + Send + Sync> Clone for Instance<A> {
    fn clone(&self) -> Self {
        Self {
            _marker: PhantomData,
            rec: self.rec.clone(),
        }
    }
}

#[Object]
impl<A: App + HasDb + Send + Sync + 'static> Instance<A> {
    async fn id(&self) -> ID {
        ID(self.rec.id.clone())
    }
    async fn name(&self) -> &str {
        &self.rec.name
    }
    async fn slug(&self) -> &str {
        &self.rec.slug
    }
    async fn public_submission_enabled(&self) -> bool {
        self.rec.public_submission_enabled
    }
    // Named `from_display_name`, not `from_name`, purely to dodge an
    // async-graphql-derive quirk: the `#[Object]` macro generates an internal
    // `__FieldIdent` enum whose variants are the *literal* resolver method
    // identifiers (no case conversion) alongside its own associated function
    // `__FieldIdent::from_name(...)` — a resolver actually named `from_name`
    // collides with that generated function name and fails to compile. The
    // GraphQL field name is unaffected (`#[graphql(name = ...)]` below).
    #[graphql(name = "fromName")]
    async fn from_display_name(&self) -> &str {
        &self.rec.from_name
    }
    async fn signature(&self) -> &str {
        &self.rec.signature
    }
    async fn created_at(&self) -> i64 {
        self.rec.created_at as i64
    }

    /// Every inbound address mapped to this instance. Owner-only: per the
    /// build plan, member invites are deferred to the CLI, and inbound
    /// addressing is treated with the same "owner, not just any member"
    /// sensitivity — an agent can work tickets without being able to
    /// reconfigure where mail routes.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::InstanceOwner(self.rec.id.clone()))")]
    async fn inbound_addresses(&self, ctx: &Context<'_>) -> Result<Vec<InboundAddressInfo>> {
        let app = ctx.data_unchecked::<Arc<A>>();
        let addrs = app
            .db()
            .list_inbound_addresses_by_instance(&self.rec.id)
            .await?;
        Ok(addrs.into_iter().map(InboundAddressInfo::from).collect())
    }

    /// Every member of this instance and their role. Owner-only. There is
    /// deliberately no way to *invite* a member over GraphQL yet — see
    /// `bin/cli.rs`'s `member add`, which is how an owner bootstraps their
    /// team today.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::InstanceOwner(self.rec.id.clone()))")]
    async fn members(&self, ctx: &Context<'_>) -> Result<Vec<MemberInfo<A>>> {
        let app = ctx.data_unchecked::<Arc<A>>();
        let memberships = app.db().list_memberships_by_instance(&self.rec.id).await?;
        if memberships.is_empty() {
            return Ok(vec![]);
        }
        let user_ids: Vec<&str> = memberships.iter().map(|m| m.user_id.as_str()).collect();
        let users = app.db().get_users(&user_ids).await?;
        Ok(memberships
            .into_iter()
            .zip(users)
            .filter_map(|(m, user)| {
                user.map(|u| MemberInfo {
                    user: User::new(u),
                    role: m.role.into(),
                })
            })
            .collect())
    }
}

/// One member of an instance: the user, and their role in this instance
/// specifically (a user's role can differ across the instances they belong
/// to).
#[derive(Debug, PartialEq)]
pub struct MemberInfo<A: App + HasDb + Send + Sync> {
    user: User<A>,
    role: MembershipRoleType,
}

impl<A: App + HasDb + Send + Sync> Clone for MemberInfo<A> {
    fn clone(&self) -> Self {
        Self {
            user: self.user.clone(),
            role: self.role,
        }
    }
}

#[Object]
impl<A: App + HasDb + Send + Sync + 'static> MemberInfo<A> {
    async fn user(&self) -> &User<A> {
        &self.user
    }
    async fn role(&self) -> MembershipRoleType {
        self.role
    }
}

/// One of the caller's own memberships: which instance, and their role in it.
/// Backs `User.memberships` (i.e. `me.memberships`).
#[derive(Debug, PartialEq)]
pub struct MembershipInfo<A: App + HasDb + Send + Sync> {
    instance: Instance<A>,
    role: MembershipRoleType,
}

impl<A: App + HasDb + Send + Sync> Clone for MembershipInfo<A> {
    fn clone(&self) -> Self {
        Self {
            instance: self.instance.clone(),
            role: self.role,
        }
    }
}

#[Object]
impl<A: App + HasDb + Send + Sync + 'static> MembershipInfo<A> {
    async fn instance(&self) -> &Instance<A> {
        &self.instance
    }
    async fn role(&self) -> MembershipRoleType {
        self.role
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

    /// Every instance this user belongs to, and their role in each — the
    /// instance switcher's data source. Same defence-in-depth guard as
    /// [`Self::passkeys`], and the same caveat: this reads `self.rec.id`
    /// directly (not the caller's own id off `AuthInfo`), so it is already
    /// correct for a future admin-facing "look up any user" query, not just
    /// `me`.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn memberships(&self, ctx: &Context<'_>) -> Result<Vec<MembershipInfo<A>>> {
        let app = ctx.data_unchecked::<Arc<A>>();
        let memberships = app.db().list_memberships_by_user(&self.rec.id).await?;
        if memberships.is_empty() {
            return Ok(vec![]);
        }
        let instance_ids: Vec<&str> = memberships.iter().map(|m| m.instance_id.as_str()).collect();
        let instances = app.db().get_instances(&instance_ids).await?;
        Ok(memberships
            .into_iter()
            .zip(instances)
            .filter_map(|(m, inst)| {
                inst.map(|i| MembershipInfo {
                    instance: Instance::new(i),
                    role: m.role.into(),
                })
            })
            .collect())
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

    /// Resolve a slug (from a URL: `/app/:slug`) to the full instance record.
    /// Requires *some* authenticated user (checked here, not via a static
    /// `#[graphql(guard)]`, since the guard's `Member(instance_id)` requirement
    /// needs an instance id this resolver only learns after resolving the
    /// slug) and, once resolved, that the caller is actually a member of that
    /// instance. A slug that doesn't resolve, or that resolves to an instance
    /// the caller isn't a member of, is reported identically — `None` — so a
    /// non-member can't use this to probe which slugs exist.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn instance(&self, ctx: &Context<'_>, slug: String) -> Result<Option<Instance<A>>> {
        let Some(AuthInfo::User { memberships, .. }) = ctx.data_opt::<AuthInfo>() else {
            return Err(ApiError::forbidden("Must be authenticated as a user").into());
        };
        let app = ctx.data_unchecked::<Arc<A>>();
        let Some(instance_id) = app.db().get_instance_id_by_slug(&slug).await? else {
            return Ok(None);
        };
        if !memberships.iter().any(|m| m.instance_id == instance_id) {
            return Ok(None);
        }
        let rec = app
            .db()
            .get_instances(&[&instance_id])
            .await?
            .into_iter()
            .next()
            .flatten()
            .filter(|i| !i.deleted)
            .ok_or_else(|| anyhow!("Instance with ID {instance_id} missing"))?;
        Ok(Some(Instance::new(rec)))
    }

    /// Every instance with public ticket submission enabled — the bare
    /// `/submit` list. Unauthenticated by design (this is how an anonymous
    /// visitor finds which organisations accept public tickets), and
    /// deliberately exposes only `name`/`slug`: see [`PublicInstance`]'s doc
    /// comment for why nothing else about a tenant may leak here.
    async fn public_instances(&self, ctx: &Context<'_>) -> Result<Vec<PublicInstance>> {
        let app = ctx.data_unchecked::<Arc<A>>();
        let instances = app.db().list_instances().await?;
        Ok(instances
            .into_iter()
            .filter(|i| i.public_submission_enabled && !i.deleted)
            .map(PublicInstance::from)
            .collect())
    }
}
