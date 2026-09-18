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

/// An `instance` row — a tenant organisation. Owns zero or more
/// [`InboundAddress`]es and has zero or more [`Membership`]s.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub slug: String,
    /// Opt-in, default `false`. Gates whether the instance appears in
    /// `publicInstances` and whether `requestSubmitCode` will issue a code for
    /// it. Per the omit-optional-attributes house rule, only ever written as
    /// `true`; `false` is the absence of the attribute.
    pub public_submission_enabled: bool,
    pub from_name: String,
    pub signature: String,
    pub created_at: u64,
    /// Soft-delete marker. Same omit convention as
    /// `public_submission_enabled`: present (and `true`) means deleted, absent
    /// means active.
    pub deleted: bool,
}

impl HasID for Instance {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Update shapes for `instance`. `Fields` never touches `deleted` (soft-delete
/// is its own variant, since it's a presence-marker attribute per the house
/// rule, not a plain field overwrite).
#[derive(Clone, Debug, PartialEq)]
pub enum InstanceUpdateShape<'a> {
    Fields {
        name: &'a str,
        from_name: &'a str,
        signature: &'a str,
        public_submission_enabled: bool,
    },
    /// Soft-delete (`true`) or restore (`false`) an instance. `true` sets the
    /// `deleted` attribute; `false` removes it — never written as `Bool(false)`,
    /// per the omit-optional-attributes house rule.
    SetDeleted(bool),
}

/// `inbound_address.kind`: whether a row matches one exact address or every
/// address at a domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressKind {
    Exact,
    Wildcard,
}

impl AddressKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Wildcard => "wildcard",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exact" => Some(Self::Exact),
            "wildcard" => Some(Self::Wildcard),
            _ => None,
        }
    }
}

/// An `inbound_address` row. Hash key is `address` itself (lowercased, or a
/// `*@domain` wildcard) — see `SCHEMA.md` for why: inbound-mail routing
/// (`crate::inbound::routing`) does a direct `GetItem` on the normalized
/// recipient, so making the address the key turns routing into a `GetItem`
/// instead of a `Query`.
#[derive(Clone, Debug, PartialEq)]
pub struct InboundAddress {
    pub address: String,
    pub instance_id: String,
    pub kind: AddressKind,
    pub created_at: u64,
}

/// `membership.role`: what a user may do in an instance. `Owner` implies every
/// `Agent` permission plus instance settings (inbound addresses, membership).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MembershipRole {
    Owner,
    Agent,
}

impl MembershipRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Agent => "agent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "owner" => Some(Self::Owner),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

/// A `membership` row: one user's role in one instance.
#[derive(Clone, Debug, PartialEq)]
pub struct Membership {
    pub id: String,
    pub user_id: String,
    pub instance_id: String,
    pub role: MembershipRole,
}

impl HasID for Membership {
    fn id(&self) -> &str {
        &self.id
    }
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

/// `ticket.status` — the un-composited form of `instance_status`'s suffix. Kept
/// alongside the composite marker attributes because resolvers read `status`
/// directly far more often than they need the composite (see `SCHEMA.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketStatus {
    Open,
    Closed,
    Deleted,
}

impl TicketStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Deleted => "deleted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }
}

/// The desired state of `ticket`'s three sparse composite marker attributes —
/// `instance_status`, `instance_visible`, `instance_assignee` — for a given
/// `(instance_id, status, assignee_user_id)`. `Some(value)` means the write
/// path must `SET` the attribute to `value`; `None` means it must `REMOVE` it
/// (never write `AttributeValue::Null` — see `CLAUDE.md`'s house rule and
/// `SCHEMA.md`'s "Known issues" callout on why a GSI hash key attribute must
/// be *absent*, not null, for a row to drop out of that index).
///
/// This is the single place that decides what the three markers *should* be.
/// [`crate::dynamodb::Handler::create_ticket`] and its `update_ticket`
/// (`TicketUpdateShape::SetStatusAndAssignee`) both build their `UpdateItem`/
/// `PutItem` calls from this function's output rather than each computing
/// their own — the bug this project is most worried about (per the build
/// plan) is exactly two write paths disagreeing about when a marker should be
/// present, and having only one function decide closes that off structurally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TicketMarkers {
    /// Always present: `"{instance_id}#{status}"`.
    pub instance_status: String,
    /// `Some("{instance_id}")` unless `status == Deleted`.
    pub instance_visible: Option<String>,
    /// `Some("{instance_id}#{assignee_user_id}")` only when assigned *and*
    /// `status != Deleted` — a deleted ticket carries no assignee marker
    /// regardless of whether `assignee_user_id` was passed, since "assigned
    /// to me" must never surface a deleted ticket.
    pub instance_assignee: Option<String>,
}

pub fn compute_ticket_markers(
    instance_id: &str,
    status: TicketStatus,
    assignee_user_id: Option<&str>,
) -> TicketMarkers {
    let instance_status = format!("{instance_id}#{}", status.as_str());
    if status == TicketStatus::Deleted {
        return TicketMarkers {
            instance_status,
            instance_visible: None,
            instance_assignee: None,
        };
    }
    TicketMarkers {
        instance_status,
        instance_visible: Some(instance_id.to_string()),
        instance_assignee: assignee_user_id.map(|uid| format!("{instance_id}#{uid}")),
    }
}

/// The `[#{slug}-{number}]` subject tag inbound-mail threading (step 7) looks
/// for, and outbound mail (step 6) stamps onto every `Subject:` header. Pure
/// and unit-tested here since both later steps depend on this exact format
/// staying stable.
pub fn ticket_subject_tag(slug: &str, number: u64) -> String {
    format!("[#{slug}-{number}]")
}

/// A `ticket` row. See `SCHEMA.md` for the full attribute-by-attribute
/// rationale, especially the three composite marker attributes
/// (`instance_status`/`instance_visible`/`instance_assignee`), which are
/// *not* fields of this struct — they exist purely as sparse GSI keys and are
/// recomputed from `status`/`assignee_user_id` by [`compute_ticket_markers`]
/// whenever a write needs them, never stored/read as ordinary data here.
#[derive(Clone, Debug, PartialEq)]
pub struct Ticket {
    pub id: String,
    pub instance_id: String,
    /// Per-instance sequential number, allocated once via
    /// [`Handler::increment_ticket_counter`]'s atomic `ADD` — never
    /// read-then-written. See `SCHEMA.md`'s "Known issues" register.
    pub number: u64,
    pub subject: String,
    pub status: TicketStatus,
    pub requester_emails: Vec<String>,
    pub cc_emails: Vec<String>,
    pub assignee_user_id: Option<String>,
    /// Opaque 16-char token embedded in outbound `Reply-To` as `+t{reply_token}`
    /// (step 6) and read back out of an inbound `+tag` recipient (step 7) for
    /// threading. Minted once at creation, never rotated.
    pub reply_token: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_activity_at: u64,
    /// Denormalised: true once any message on this ticket carries an
    /// attachment. Exists so a list row can show a paperclip without reading
    /// every message of every ticket on the page — which is one extra query
    /// per row, on the screen agents look at most. Never cleared: an
    /// attachment that existed is a fact about the thread's history.
    pub has_attachments: bool,
}

impl HasID for Ticket {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Update shapes for `ticket`. **`SetStatusAndAssignee` is the only variant
/// allowed to touch any of the three composite marker attributes**, and it
/// always recomputes all three together (via [`compute_ticket_markers`]) in a
/// single `UpdateItem` — every call site that changes status (open/close/
/// delete/restore) or assignee (assign/unassign) goes through this one
/// variant, passing through whichever of the two it isn't changing. This is
/// deliberate: a write path that updated `instance_status` in one call and
/// `instance_assignee` in a separate one would reintroduce the exact race
/// `SCHEMA.md`'s "Known issues" register warns about (a crash between the two
/// calls leaving the markers inconsistent) — collapsing every marker-affecting
/// write into this single variant makes that structurally impossible.
///
/// The remaining variants never touch the markers, only ordinary fields, and
/// still bump `updated_at`/`last_activity_at` — any write to a ticket counts
/// as activity for the "newest activity first" ordering the listing GSIs use.
#[derive(Clone, Debug, PartialEq)]
pub enum TicketUpdateShape<'a> {
    SetStatusAndAssignee {
        instance_id: &'a str,
        status: TicketStatus,
        /// The *desired* assignee after this write — pass the ticket's
        /// current assignee unchanged for a pure status transition, or the
        /// current status unchanged for a pure assignment change.
        assignee_user_id: Option<&'a str>,
        now: u64,
    },
    AddRequester {
        email: &'a str,
        now: u64,
    },
    RemoveRequester {
        email: &'a str,
        now: u64,
    },
    AddCc {
        email: &'a str,
        now: u64,
    },
    RemoveCc {
        email: &'a str,
        now: u64,
    },
    /// Bump `updated_at`/`last_activity_at` only — a new message (reply/note)
    /// landed on the ticket with no status or assignee change.
    Touch {
        now: u64,
    },
    /// Record that this ticket has at least one attachment somewhere in its
    /// thread. Idempotent, and one-way: see [`Ticket::has_attachments`].
    MarkHasAttachments,
}

/// Keyset pagination cursor for a ticket listing: `{last_activity_at}:{id}`,
/// mirroring seslogin's `PeriodCursor`. `last_activity_at` first (the sort
/// key every listing GSI shares) so ties on it still resolve deterministically
/// via `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TicketCursor {
    pub last_activity_at: u64,
    pub id: String,
}

/// Which of the three listing GSIs a `list_tickets` call queries, and any
/// extra non-key filter it needs. Maps directly to the four `TicketStatusFilter`
/// values the GraphQL layer exposes:
///
/// - `OPEN`/`CLOSED`/`DELETED` (owner-only, enforced in the resolver, not
///   here) → `Status(_)`, the `instance_status-last_activity_at-index`.
/// - `ALL` → `Visible`, the `instance_visible-last_activity_at-index` (every
///   non-deleted ticket).
/// - `assignedTo` set (regardless of `status`) → `AssignedTo`, the
///   `instance_assignee-last_activity_at-index`, which is itself sparse to
///   assigned + non-deleted tickets; an additional `status` narrows further
///   via a `FilterExpression` (not a key condition — DynamoDB allows only one
///   index per query).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TicketListFilter {
    Status(TicketStatus),
    Visible,
    AssignedTo {
        user_id: String,
        status: Option<TicketStatus>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListTicketsPage {
    pub after: Option<TicketCursor>,
    pub before: Option<TicketCursor>,
    pub limit: i32,
    /// `true` for the default "newest activity first" order.
    pub descending: bool,
}

/// `ticket_message.kind`. `Note` rows are internal-only: never emailed, and
/// filtered out of any requester-visible resolver (enforced in
/// `graphql::query::Ticket::messages`, not only in the web UI).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketMessageKind {
    Inbound,
    Reply,
    Note,
    System,
}

impl TicketMessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Reply => "reply",
            Self::Note => "note",
            Self::System => "system",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "inbound" => Some(Self::Inbound),
            "reply" => Some(Self::Reply),
            "note" => Some(Self::Note),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// One `ticket_message.attachments` entry — populated by inbound-mail
/// storage and by `replyToTicket`'s `attachmentKeys` (both step 7), via
/// `TicketMessageUpdateShape::SetAttachments`. See `SCHEMA.md`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub s3_key: String,
    pub filename: String,
    pub content_type: String,
    pub size: u64,
}

/// A `ticket_message` row. See `SCHEMA.md` for the full rationale, especially
/// why `author_user_id`/`from_email` and `body_text`/`body_html` are each
/// mutually-optional-but-usually-one-present pairs rather than a single
/// tagged field.
#[derive(Clone, Debug, PartialEq)]
pub struct TicketMessage {
    pub id: String,
    pub ticket_id: String,
    pub kind: TicketMessageKind,
    /// Present for `reply`/`note`; absent for `inbound`/`system`.
    pub author_user_id: Option<String>,
    /// Present for `inbound`; absent otherwise.
    pub from_email: Option<String>,
    pub to_emails: Vec<String>,
    pub cc_emails: Vec<String>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    /// SES's rewritten `Message-ID` once this message is actually sent (step
    /// 6) or the id an inbound message arrived with (step 7). Absent until
    /// [`Handler::update_ticket_message`]'s `SetRfcMessageId` lands it — the
    /// row is always created first (see `graphql::mutations::reply_to_ticket`),
    /// before the send even happens, so this is genuinely absent, not just
    /// unset-by-convention, for a reply that hasn't gone out yet or whose
    /// send failed.
    pub rfc_message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    /// See [`Attachment`]'s doc comment — always empty as of this step.
    pub attachments: Vec<Attachment>,
    /// Present only on `inbound` rows, pointing at the raw MIME in S3 (step 7).
    pub raw_s3_key: Option<String>,
    pub created_at: u64,
}

impl HasID for TicketMessage {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Update shapes for `ticket_message`. Only `rfc_message_id` is ever
/// updated after creation — every other attribute is fixed at
/// [`Handler::create_ticket_message`] time — because it's the one thing
/// that can't be known until *after* the row exists: a reply is persisted
/// first, then sent, then (only on success) stamped with the id SES
/// returned. See `graphql::mutations::reply_to_ticket`'s doc comment for
/// why the row is never rolled back when the send itself fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TicketMessageUpdateShape<'a> {
    SetRfcMessageId {
        rfc_message_id: &'a str,
    },
    /// Stamp the raw MIME's S3 key onto an `inbound` row. Set after the row
    /// is created for the same reason `rfc_message_id` is: the pipeline
    /// creates the row, then stores the raw bytes (or, for the Lambda, has
    /// already relied on SES having stored them, and only needs to record
    /// the key), so this can't be known at `create_ticket_message` time
    /// without reordering the pipeline in a way that would leave a
    /// half-written row on a storage failure.
    SetRawS3Key {
        raw_s3_key: &'a str,
    },
    /// Stamp the final `{s3_key, filename, content_type, size}` list onto a
    /// message once every attachment has been uploaded (inbound) or moved
    /// out of `pending/` (`replyToTicket`) — see [`Attachment`]'s doc
    /// comment. An empty slice `REMOVE`s the attribute per the
    /// omit-optional-attributes house rule.
    SetAttachments {
        attachments: &'a [Attachment],
    },
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
    // ── instance ──────────────────────────────────────────────────────────
    fn get_instances<T: AsRef<str> + Sync>(
        &self,
        ids: &[T],
    ) -> impl Future<Output = Result<Vec<Option<Instance>>>> + Send;
    /// Resolve a slug to its instance id via `slug-index`, collapsed through
    /// [`at_most_one`]. Slug uniqueness is enforced at the application layer
    /// (DynamoDB only enforces uniqueness on the primary key) — see
    /// [`Handler::create_instance`]'s doc comment and `SCHEMA.md`'s known-issues
    /// register for the race this leaves open.
    fn get_instance_id_by_slug(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<Option<String>>> + Send;
    /// Create an instance. Callers (the GraphQL layer, the CLI) are expected to
    /// call [`Handler::get_instance_id_by_slug`] first and reject a taken slug
    /// before calling this — this method itself does not re-check, so a
    /// concurrent pair of callers that both pass that pre-check can still both
    /// succeed here, leaving two instances that share a slug. That race is
    /// documented (not fixed) in `SCHEMA.md`'s known-issues register: instance
    /// creation is a rare, operator-driven action (CLI bootstrap), not a
    /// high-concurrency user-facing path, so the explicit pre-check plus a
    /// documented race was chosen over a deterministic-id-from-slug scheme
    /// (which would break if a slug is ever renamed, since `id` is the stable
    /// foreign key every other table references).
    fn create_instance(
        &self,
        name: &str,
        slug: &str,
        from_name: &str,
        signature: &str,
        public_submission_enabled: bool,
    ) -> impl Future<Output = Result<Instance>> + Send;
    fn update_instance(
        &self,
        id: &str,
        change: InstanceUpdateShape<'_>,
    ) -> impl Future<Output = Result<()>> + Send;
    /// Every instance, active and deleted alike (callers filter as needed) — a
    /// base-table scan, not a query. Fine at this table's scale: instances are
    /// tenant organisations, created by an operator via the CLI, not a
    /// high-cardinality user-generated table.
    fn list_instances(&self) -> impl Future<Output = Result<Vec<Instance>>> + Send;

    // ── inbound_address ──────────────────────────────────────────────────────
    /// `address` must already be normalized (lowercased; `*@domain` for a
    /// wildcard) — see `crate::inbound::routing` for the normalization/
    /// classification logic callers are expected to run first.
    fn create_inbound_address(
        &self,
        address: &str,
        instance_id: &str,
        kind: AddressKind,
    ) -> impl Future<Output = Result<InboundAddress>> + Send;
    fn get_inbound_address(
        &self,
        address: &str,
    ) -> impl Future<Output = Result<Option<InboundAddress>>> + Send;
    fn delete_inbound_address(&self, address: &str) -> impl Future<Output = Result<()>> + Send;
    fn list_inbound_addresses_by_instance(
        &self,
        instance_id: &str,
    ) -> impl Future<Output = Result<Vec<InboundAddress>>> + Send;

    // ── membership ────────────────────────────────────────────────────────
    fn create_membership(
        &self,
        user_id: &str,
        instance_id: &str,
        role: MembershipRole,
    ) -> impl Future<Output = Result<Membership>> + Send;
    fn delete_membership(&self, id: &str) -> impl Future<Output = Result<()>> + Send;
    fn list_memberships_by_user(
        &self,
        user_id: &str,
    ) -> impl Future<Output = Result<Vec<Membership>>> + Send;
    fn list_memberships_by_instance(
        &self,
        instance_id: &str,
    ) -> impl Future<Output = Result<Vec<Membership>>> + Send;

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
    /// Every user — a base-table scan, not a query, for the same reason as
    /// [`Handler::list_instances`]: `user` has no listing GSI (only
    /// `email-index`, for the login path), and this table's cardinality is a
    /// handful of team members, not a user-generated table. Backs
    /// `bin/cli.rs`'s `user list`.
    fn list_users(&self) -> impl Future<Output = Result<Vec<User>>> + Send;
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

    // ── ticket ────────────────────────────────────────────────────────────
    fn get_tickets<T: AsRef<str> + Sync>(
        &self,
        ids: &[T],
    ) -> impl Future<Output = Result<Vec<Option<Ticket>>>> + Send;
    /// Atomically allocate the next per-instance ticket number via `UpdateItem
    /// ADD` on `counter` (`id = instance_id`) — never a read-then-write. See
    /// `SCHEMA.md`'s "Known issues" register for the duplicate-number race
    /// this closes: a failure after this call succeeds but before the ticket
    /// row is written leaves a *gap*, never a *duplicate*, since the counter
    /// itself never moves backward.
    fn increment_ticket_counter(
        &self,
        instance_id: &str,
    ) -> impl Future<Output = Result<u64>> + Send;
    /// Create a new, always-`Open`, always-unassigned ticket. `number` must
    /// come from a prior [`Handler::increment_ticket_counter`] call —
    /// callers, not this method, own the counter/ticket-write sequencing.
    fn create_ticket(
        &self,
        instance_id: &str,
        number: u64,
        subject: &str,
        requester_emails: &[String],
        cc_emails: &[String],
    ) -> impl Future<Output = Result<Ticket>> + Send;
    fn update_ticket(
        &self,
        id: &str,
        change: TicketUpdateShape<'_>,
    ) -> impl Future<Output = Result<()>> + Send;
    /// One page of a ticket listing. See [`TicketListFilter`] for which GSI
    /// each variant queries and [`ListTicketsPage`]/[`TicketCursor`] for the
    /// keyset-pagination shape (mirrors seslogin's `list_periods_for_location`).
    fn list_tickets(
        &self,
        instance_id: &str,
        filter: TicketListFilter,
        page: ListTicketsPage,
    ) -> impl Future<Output = Result<Vec<Ticket>>> + Send;
    /// Resolve `{instance_id}#{number}` via `instance_number-index` to a
    /// ticket id — the `[#{slug}-{number}]` subject-tag threading fallback
    /// (step 7). `number` alone, paired with the *already-resolved*
    /// `instance_id` (not the subject's slug, which is cosmetic once an
    /// instance is known), is what the index is keyed on — see
    /// `inbound::resolution::ResolutionPlan::subject_number`'s doc comment
    /// for why this makes the result structurally incapable of resolving to
    /// another tenant's ticket. Collapsed through [`at_most_one`]: the
    /// composite key is meant to be unique by construction (one ticket per
    /// instance+number), so more than one hit is a data-integrity error, not
    /// a normal outcome. Callers still `GetItem` the returned id afterward
    /// (this is a `KEYS_ONLY` index) for a strongly consistent read.
    fn get_ticket_id_by_instance_number(
        &self,
        instance_id: &str,
        number: u64,
    ) -> impl Future<Output = Result<Option<String>>> + Send;

    // ── ticket_message ───────────────────────────────────────────────────
    /// `in_reply_to`/`references` are this new message's *own* threading
    /// headers — for a reply/notice/acknowledgement (step 6), computed by
    /// `outbound::threading_for` from the ticket's prior messages before
    /// this one is built; for an inbound message (step 7), copied from the
    /// arriving mail's own headers. Neither is the row's `rfc_message_id`
    /// (this message's *own* identity once sent) — that lands afterward via
    /// [`Handler::update_ticket_message`], since it isn't known until the
    /// send actually happens.
    #[allow(clippy::too_many_arguments)]
    fn create_ticket_message(
        &self,
        ticket_id: &str,
        kind: TicketMessageKind,
        author_user_id: Option<&str>,
        from_email: Option<&str>,
        to_emails: &[String],
        cc_emails: &[String],
        body_text: Option<&str>,
        body_html: Option<&str>,
        in_reply_to: Option<&str>,
        references: Option<&str>,
    ) -> impl Future<Output = Result<TicketMessage>> + Send;
    /// Every message for a ticket, oldest first (the GSI's natural ascending
    /// scan order) — the thread view's data source.
    fn list_ticket_messages(
        &self,
        ticket_id: &str,
    ) -> impl Future<Output = Result<Vec<TicketMessage>>> + Send;
    fn update_ticket_message(
        &self,
        id: &str,
        change: TicketMessageUpdateShape<'_>,
    ) -> impl Future<Output = Result<()>> + Send;
    /// Resolve an `In-Reply-To`/`References` id to the ticket it belongs to,
    /// via `rfc_message_id-index` (a `KEYS_ONLY` index projecting the
    /// `ticket_message`'s own id, not `ticket_id`) followed by a strongly
    /// consistent `GetItem` on that message to read `ticket_id` back out —
    /// the `In-Reply-To`/`References` threading fallback (step 7). `None`
    /// when no message carries that id. Collapsed through [`at_most_one`]:
    /// `rfc_message_id` is meant to be unique (it's the provider's own
    /// message id), so more than one hit is a data-integrity error.
    fn get_ticket_id_by_rfc_message_id(
        &self,
        rfc_message_id: &str,
    ) -> impl Future<Output = Result<Option<String>>> + Send;

    // ── processed_message ─────────────────────────────────────────────────
    /// Inbound-mail idempotency: a conditional `PutItem`
    /// (`attribute_not_exists(ses_message_id)`) written *before* any other
    /// processing of a given SES message id. Returns `true` when this call
    /// is the one that claimed it (processing should proceed); `false` when
    /// another call already claimed it (a duplicate SQS delivery — the
    /// caller exits successfully without reprocessing). See `SCHEMA.md`'s
    /// "Known issues" register for the window this does and does not close:
    /// marking *before* the work means a crash mid-processing drops the
    /// message rather than duplicating it, a deliberate trade (SES retries
    /// are common; double-posting a customer reply into a ticket is worse
    /// than a rare dropped message that stays in S3 and the DLQ).
    fn claim_processed_message(
        &self,
        ses_message_id: &str,
        now: u64,
        expires_at: u64,
    ) -> impl Future<Output = Result<bool>> + Send;

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

    #[test]
    fn ticket_status_as_str_parse_round_trips() {
        for status in [
            TicketStatus::Open,
            TicketStatus::Closed,
            TicketStatus::Deleted,
        ] {
            assert_eq!(TicketStatus::parse(status.as_str()), Some(status));
        }
    }

    #[test]
    fn ticket_status_parse_rejects_garbage() {
        assert_eq!(TicketStatus::parse("archived"), None);
        assert_eq!(TicketStatus::parse(""), None);
        assert_eq!(TicketStatus::parse("Open"), None);
    }

    #[test]
    fn ticket_message_kind_as_str_parse_round_trips() {
        for kind in [
            TicketMessageKind::Inbound,
            TicketMessageKind::Reply,
            TicketMessageKind::Note,
            TicketMessageKind::System,
        ] {
            assert_eq!(TicketMessageKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn ticket_message_kind_parse_rejects_garbage() {
        assert_eq!(TicketMessageKind::parse("bogus"), None);
    }

    #[test]
    fn ticket_subject_tag_format() {
        assert_eq!(ticket_subject_tag("acme", 42), "[#acme-42]");
        assert_eq!(ticket_subject_tag("ridgeline", 1), "[#ridgeline-1]");
    }

    /// The marker-attribute computation for every `(status, assignee)`
    /// combination — the exhaustive table this whole design lives or dies on.
    /// See `compute_ticket_markers`'s doc comment for why both write paths
    /// (`create_ticket`, `update_ticket`'s `SetStatusAndAssignee`) must go
    /// through this one function rather than each recomputing it.
    #[test]
    fn markers_open_unassigned() {
        let m = compute_ticket_markers("inst1", TicketStatus::Open, None);
        assert_eq!(m.instance_status, "inst1#open");
        assert_eq!(m.instance_visible.as_deref(), Some("inst1"));
        assert_eq!(m.instance_assignee, None);
    }

    #[test]
    fn markers_open_assigned() {
        let m = compute_ticket_markers("inst1", TicketStatus::Open, Some("user1"));
        assert_eq!(m.instance_status, "inst1#open");
        assert_eq!(m.instance_visible.as_deref(), Some("inst1"));
        assert_eq!(m.instance_assignee.as_deref(), Some("inst1#user1"));
    }

    #[test]
    fn markers_closed_unassigned() {
        let m = compute_ticket_markers("inst1", TicketStatus::Closed, None);
        assert_eq!(m.instance_status, "inst1#closed");
        assert_eq!(m.instance_visible.as_deref(), Some("inst1"));
        assert_eq!(m.instance_assignee, None);
    }

    #[test]
    fn markers_closed_assigned() {
        let m = compute_ticket_markers("inst1", TicketStatus::Closed, Some("user1"));
        assert_eq!(m.instance_status, "inst1#closed");
        assert_eq!(m.instance_visible.as_deref(), Some("inst1"));
        assert_eq!(m.instance_assignee.as_deref(), Some("inst1#user1"));
    }

    #[test]
    fn markers_deleted_unassigned() {
        let m = compute_ticket_markers("inst1", TicketStatus::Deleted, None);
        assert_eq!(m.instance_status, "inst1#deleted");
        assert_eq!(m.instance_visible, None);
        assert_eq!(m.instance_assignee, None);
    }

    /// The case that makes `Deleted` its own branch rather than falling out of
    /// the same logic as `Open`/`Closed`: a deleted-but-still-assigned ticket
    /// must drop `instance_assignee` too, not just `instance_visible` — "assigned
    /// to me" must never surface a deleted ticket.
    #[test]
    fn markers_deleted_assigned_still_drops_assignee_marker() {
        let m = compute_ticket_markers("inst1", TicketStatus::Deleted, Some("user1"));
        assert_eq!(m.instance_status, "inst1#deleted");
        assert_eq!(m.instance_visible, None);
        assert_eq!(
            m.instance_assignee, None,
            "a deleted ticket must never carry an instance_assignee marker, even if it has an assignee_user_id"
        );
    }
}
