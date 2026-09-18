//! `MutationRoot`: email-code login, opaque token issuance/revocation, and
//! passkeys. Ported from seslogin's `graphql/mutations.rs` (the auth block
//! around its lines 360-570, and the passkey block around 2146-2560), trimmed to
//! microticket's single `User` principal — no kiosk sessions, no API tokens.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_graphql::{Context, ID, Object, SimpleObject};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use tracing::{info, warn};

use crate::app::{App, HasDb, HasMail};
use crate::auth::{self, AuthInfo};
use crate::db;
use crate::db::Handler as _;
use crate::inbound::routing;
use crate::mail::Handler as _;

use super::auth::{AuthGuard, AuthRequirement, is_member};
use super::error::ApiError;
use super::query::{InboundAddressInfo, PasskeyInfo, Ticket, TicketMessage, TicketStatusType};

/// One code per address per this many seconds — cheap anti-spam for
/// `requestAuthCode`, independent of the code's own 10-minute validity.
const LOGIN_CODE_RATE_LIMIT_S: u64 = 30;
/// A code is burned (deleted, forcing a fresh `requestAuthCode`) after this many
/// wrong guesses.
const LOGIN_CODE_MAX_ATTEMPTS: u64 = 5;
/// Decimal digits in a login code.
const LOGIN_CODE_DIGITS: u32 = 6;
/// Passkeys a single user may register. Matches seslogin's cap; re-checked after
/// the WebAuthn ceremony completes (see `finish_passkey_registration`) to close
/// the race between the pre-check and the write.
const MAX_PASSKEYS_PER_USER: usize = 10;

fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// The calling `AuthInfo::User`'s id, or a `Forbidden` error for anything else
/// (no credentials, or a `Requester` capability token — passkeys belong to
/// users, not to the public submit flow).
fn require_user_id(ctx: &Context<'_>) -> Result<String> {
    match ctx.data_opt::<AuthInfo>() {
        Some(AuthInfo::User { id, .. }) => Ok(id.clone()),
        _ => Err(ApiError::forbidden("Must be authenticated as a user").into()),
    }
}

/// The per-ticket authorization check every ticket mutation uses: fetch the
/// ticket by id, then verify the caller is a member of *its* `instance_id`.
///
/// This is deliberately not "check the caller is a member of an `instanceId`
/// argument" — these mutations take only `ticketId`, and a ticket's instance
/// is a fact of the record, not something the caller gets to assert. Checking
/// an `instanceId` argument instead (or trusting one, if a resolver even had
/// one to check) would let a member of instance A act on instance B's ticket
/// simply by passing B's ticket id; fetching the record and checking *its*
/// `instance_id` closes that off structurally. A ticket that doesn't exist,
/// and a ticket that exists but belongs to an instance the caller isn't a
/// member of, are reported identically — `NOT_FOUND` — so this can't be used
/// to probe which ticket ids exist in another instance.
async fn require_ticket_member<A: App + HasDb + HasMail + Send + Sync>(
    ctx: &Context<'_>,
    app: &A,
    ticket_id: &str,
) -> Result<db::Ticket> {
    let ticket = app
        .db()
        .get_tickets(&[ticket_id])
        .await?
        .into_iter()
        .next()
        .flatten()
        .ok_or_else(|| ApiError::not_found("Ticket", ticket_id))?;
    let Some(AuthInfo::User { memberships, .. }) = ctx.data_opt::<AuthInfo>() else {
        return Err(ApiError::forbidden("Must be authenticated as a user").into());
    };
    if !is_member(memberships, &ticket.instance_id) {
        return Err(ApiError::not_found("Ticket", ticket_id).into());
    }
    Ok(ticket)
}

/// Trim, lowercase, and sanity-check an email address supplied to
/// `addTicketRequester`/`addTicketCc` — a minimal shape check (one `@`,
/// non-empty local/domain parts), not full RFC 5321 validation, matching how
/// little validation `inbound::routing::classify_for_storage` does for the
/// same reason: perfect email validation is famously not worth attempting,
/// and DynamoDB/SES will reject anything that matters more than this does.
fn normalize_ticket_email(raw: &str) -> Result<String> {
    let lower = raw.trim().to_lowercase();
    let Some((local, domain)) = lower.split_once('@') else {
        return Err(anyhow!(
            "{raw:?} is not a valid email address (missing '@')"
        ));
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return Err(anyhow!("{raw:?} is not a valid email address"));
    }
    Ok(lower)
}

pub struct MutationRoot<A: App + HasDb + HasMail + Send + Sync> {
    pub(super) app: Arc<A>,
}

#[derive(SimpleObject)]
struct PasskeyChallenge {
    challenge_id: String,
    options_json: String,
}

#[Object]
impl<A: App + HasDb + HasMail + Send + Sync + 'static> MutationRoot<A> {
    /// Request an email login code. **Always returns `true`**, whether or not the
    /// address belongs to a real, enabled user — telling the caller otherwise
    /// would let anyone enumerate registered emails one guess at a time. Every
    /// early-return path below exists to preserve that: a bad Turnstile token, an
    /// unknown/disabled user, and a rate-limit hit are all indistinguishable from
    /// the outside.
    async fn request_auth_code(
        &self,
        ctx: &Context<'_>,
        email: String,
        turnstile_token: Option<String>,
    ) -> bool {
        let remote_ip = ctx
            .data_opt::<super::ClientIp>()
            .and_then(|ip| ip.0.as_deref());
        match crate::turnstile::verify(turnstile_token.as_deref(), remote_ip).await {
            Ok(true) => {}
            Ok(false) => {
                info!("Turnstile challenge failed for request_auth_code");
                return true;
            }
            Err(e) => {
                warn!("Turnstile error in request_auth_code: {:#}", e);
                return true;
            }
        }

        let user_id = match self.app.db().get_user_id_by_email(&email).await {
            Ok(Some(id)) => id,
            Ok(None) => return true,
            Err(e) => {
                warn!("DB error looking up user in request_auth_code: {:#}", e);
                return true;
            }
        };

        match self.app.db().get_users(&[&user_id]).await {
            Ok(users) => match users.into_iter().next().flatten() {
                Some(user) if user.enabled => {}
                _ => {
                    info!("request_auth_code: user disabled or missing id={}", user_id);
                    return true;
                }
            },
            Err(e) => {
                warn!(
                    "DB error checking user enabled in request_auth_code: {:#}",
                    e
                );
                return true;
            }
        }

        let now = crate::clock::now_sec();

        // Rate limit: at most one code per LOGIN_CODE_RATE_LIMIT_S per email.
        if let Ok(Some(existing)) = self.app.db().get_login_code(&email).await
            && now < existing.last_sent_at + LOGIN_CODE_RATE_LIMIT_S
        {
            info!("Rate limit hit for request_auth_code email={}", email);
            return true;
        }

        let code = crate::nonce::generate_code(LOGIN_CODE_DIGITS);

        // Log only in debug builds, so a code never reaches a production log.
        #[cfg(debug_assertions)]
        info!("Email login code for email={}: {}", email, code);

        let code_hash = sha256_hex(&code);
        let expires_at = crate::expire::ExpirePolicy::LoginCode.expires_at(now);

        if let Err(e) = self
            .app
            .db()
            .put_login_code(&email, &code_hash, expires_at, now)
            .await
        {
            warn!("Failed to store login code: {:#}", e);
            return true;
        }

        let subject = "Your microticket login code";
        let body = format!(
            "Your login code is: {code}\n\n\
             This code expires in 10 minutes. Do not share it.\n\n\
             If you did not request this code, you can ignore this email."
        );

        info!(user_id = %user_id, "Sending login code to {}", email);
        if let Err(e) = self
            .app
            .mail()
            .send_plain_text(&email, subject, &body)
            .await
        {
            warn!("Failed to send login code email to {}: {:#}", email, e);
        }

        true
    }

    /// Verify an email login code and return an opaque `mtu_` token on success.
    /// **Returns `null` on every failure path** — expired, wrong code, too many
    /// attempts, unknown/disabled user — so the response never tells the caller
    /// which step failed.
    async fn verify_auth_code(&self, email: String, code: String) -> Option<String> {
        let now = crate::clock::now_sec();

        let record = match self.app.db().get_login_code(&email).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                info!("verify_auth_code: no code for email={}", email);
                return None;
            }
            Err(e) => {
                warn!("DB error in verify_auth_code: {:#}", e);
                return None;
            }
        };

        if now >= record.expires_at {
            let _ = self.app.db().delete_login_code(&email).await;
            info!("verify_auth_code: expired code for email={}", email);
            return None;
        }

        if record.attempts >= LOGIN_CODE_MAX_ATTEMPTS {
            let _ = self.app.db().delete_login_code(&email).await;
            info!("verify_auth_code: too many attempts for email={}", email);
            return None;
        }

        // Count this attempt *before* comparing — a burst of guesses still burns
        // down toward LOGIN_CODE_MAX_ATTEMPTS even if every one of them errors out
        // some other way before reaching the comparison below.
        let _ = self.app.db().increment_login_code_attempts(&email).await;

        let expected_hash = sha256_hex(&code);
        if record.code_hash != expected_hash {
            info!("verify_auth_code: wrong code for email={}", email);
            return None;
        }

        let _ = self.app.db().delete_login_code(&email).await;

        let user_id = match self.app.db().get_user_id_by_email(&email).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                warn!("verify_auth_code: user not found for email={}", email);
                return None;
            }
            Err(e) => {
                warn!("DB error fetching user in verify_auth_code: {:#}", e);
                return None;
            }
        };

        match self.app.db().get_users(&[&user_id]).await {
            Ok(users) => match users.into_iter().next().flatten() {
                Some(user) if user.enabled => {}
                _ => {
                    info!("verify_auth_code: user disabled or missing id={}", user_id);
                    return None;
                }
            },
            Err(e) => {
                warn!(
                    "DB error checking user enabled in verify_auth_code: {:#}",
                    e
                );
                return None;
            }
        }

        match auth::issue_user_token(&*self.app, &user_id).await {
            Ok(token) => {
                info!("Issued user token for user_id={}", user_id);
                Some(token)
            }
            Err(e) => {
                warn!("Failed to issue user token: {:#}", e);
                None
            }
        }
    }

    /// Request a public-submit-form email verification code for `(slug,
    /// email)`. **Always returns `true`**, same anti-enumeration rationale as
    /// [`Self::request_auth_code`] — a bad Turnstile token, an unresolvable
    /// slug, and an instance with public submission disabled are all
    /// indistinguishable from the outside. The code itself is stored in
    /// `ephemeral_state` under `kind: "submit_code"` — never in `login_code`;
    /// see `auth::SUBMIT_CODE_STATE_KIND`'s doc comment for why that
    /// separation is load-bearing, not incidental.
    async fn request_submit_code(
        &self,
        ctx: &Context<'_>,
        slug: String,
        email: String,
        turnstile_token: Option<String>,
    ) -> bool {
        let remote_ip = ctx
            .data_opt::<super::ClientIp>()
            .and_then(|ip| ip.0.as_deref());
        match crate::turnstile::verify(turnstile_token.as_deref(), remote_ip).await {
            Ok(true) => {}
            Ok(false) => {
                info!("Turnstile challenge failed for request_submit_code");
                return true;
            }
            Err(e) => {
                warn!("Turnstile error in request_submit_code: {:#}", e);
                return true;
            }
        }

        let instance_id = match self.app.db().get_instance_id_by_slug(&slug).await {
            Ok(Some(id)) => id,
            Ok(None) => return true,
            Err(e) => {
                warn!("DB error resolving slug in request_submit_code: {:#}", e);
                return true;
            }
        };

        let instance = match self.app.db().get_instances(&[&instance_id]).await {
            Ok(instances) => match instances.into_iter().next().flatten() {
                Some(i) if i.public_submission_enabled && !i.deleted => i,
                _ => {
                    info!(
                        "request_submit_code: instance not public or missing id={}",
                        instance_id
                    );
                    return true;
                }
            },
            Err(e) => {
                warn!("DB error fetching instance in request_submit_code: {:#}", e);
                return true;
            }
        };

        let now = crate::clock::now_sec();
        let state_id = auth::submit_code_state_id(&instance_id, &email);

        // Rate limit: at most one code per LOGIN_CODE_RATE_LIMIT_S per
        // (instance, email) — same window as the login-code flow.
        if let Ok(Some(existing)) = self.app.db().get_ephemeral_state(&state_id).await
            && existing.kind == auth::SUBMIT_CODE_STATE_KIND
            && let Ok(payload) = serde_json::from_str::<auth::SubmitCodePayload>(&existing.payload)
            && now < payload.last_sent_at + LOGIN_CODE_RATE_LIMIT_S
        {
            info!(
                "Rate limit hit for request_submit_code instance={} email={}",
                instance_id, email
            );
            return true;
        }

        let code = crate::nonce::generate_code(LOGIN_CODE_DIGITS);

        // Log only in debug builds, so a code never reaches a production log.
        #[cfg(debug_assertions)]
        info!(
            "Submit code for instance slug={} email={}: {}",
            slug, email, code
        );

        let payload = match serde_json::to_string(&auth::SubmitCodePayload {
            code_hash: sha256_hex(&code),
            attempts: 0,
            last_sent_at: now,
        }) {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to serialize submit code payload: {:#}", e);
                return true;
            }
        };
        let expires_at = crate::expire::ExpirePolicy::SubmitCode.expires_at(now);

        if let Err(e) = self
            .app
            .db()
            .put_ephemeral_state(
                &state_id,
                auth::SUBMIT_CODE_STATE_KIND,
                &payload,
                expires_at,
            )
            .await
        {
            warn!("Failed to store submit code: {:#}", e);
            return true;
        }

        let subject = format!("Your {} verification code", instance.name);
        let body = format!(
            "Your verification code is: {code}\n\n\
             This code expires in 10 minutes. Do not share it.\n\n\
             If you did not request this code, you can ignore this email."
        );

        info!(instance_id = %instance_id, "Sending submit code to {}", email);
        if let Err(e) = self
            .app
            .mail()
            .send_plain_text(&email, &subject, &body)
            .await
        {
            warn!("Failed to send submit code email to {}: {:#}", email, e);
        }

        true
    }

    /// Verify a public-submit-form code and return a short-lived (15 minute)
    /// capability token scoped to exactly `(email, instance_id)` on success —
    /// authorised for `submitTicket` alone, never anything a real user
    /// session can do. **Returns `null` on every failure path**, mirroring
    /// [`Self::verify_auth_code`]. Reads and writes `ephemeral_state` under
    /// `kind: "submit_code"` exclusively — never `login_code` — which is what
    /// makes it structurally impossible for a code minted by
    /// [`Self::request_auth_code`] to be accepted here, or vice versa; see
    /// `tests/submit_code_dynamodb_local.rs` for the regression tests.
    async fn verify_submit_code(
        &self,
        slug: String,
        email: String,
        code: String,
    ) -> Option<String> {
        let instance_id = match self.app.db().get_instance_id_by_slug(&slug).await {
            Ok(Some(id)) => id,
            Ok(None) => {
                info!("verify_submit_code: unknown slug={}", slug);
                return None;
            }
            Err(e) => {
                warn!("DB error resolving slug in verify_submit_code: {:#}", e);
                return None;
            }
        };

        let now = crate::clock::now_sec();
        let state_id = auth::submit_code_state_id(&instance_id, &email);

        let state = match self.app.db().get_ephemeral_state(&state_id).await {
            Ok(Some(s)) if s.kind == auth::SUBMIT_CODE_STATE_KIND => s,
            Ok(_) => {
                info!(
                    "verify_submit_code: no submit code for instance={} email={}",
                    instance_id, email
                );
                return None;
            }
            Err(e) => {
                warn!("DB error in verify_submit_code: {:#}", e);
                return None;
            }
        };

        if now >= state.expires_at {
            let _ = self.app.db().delete_ephemeral_state(&state_id).await;
            info!("verify_submit_code: expired code for email={}", email);
            return None;
        }

        let Ok(mut payload) = serde_json::from_str::<auth::SubmitCodePayload>(&state.payload)
        else {
            warn!("verify_submit_code: corrupt payload for id={}", state_id);
            return None;
        };

        if payload.attempts >= LOGIN_CODE_MAX_ATTEMPTS {
            let _ = self.app.db().delete_ephemeral_state(&state_id).await;
            info!("verify_submit_code: too many attempts for email={}", email);
            return None;
        }

        // Count this attempt *before* comparing, mirroring verify_auth_code.
        payload.attempts += 1;
        if let Ok(updated) = serde_json::to_string(&payload) {
            let _ = self
                .app
                .db()
                .put_ephemeral_state(
                    &state_id,
                    auth::SUBMIT_CODE_STATE_KIND,
                    &updated,
                    state.expires_at,
                )
                .await;
        }

        if payload.code_hash != sha256_hex(&code) {
            info!("verify_submit_code: wrong code for email={}", email);
            return None;
        }

        let _ = self.app.db().delete_ephemeral_state(&state_id).await;

        match auth::issue_submit_token(&*self.app, &email, &instance_id).await {
            Ok(token) => {
                info!(
                    "Issued submit token for instance={} email={}",
                    instance_id, email
                );
                Some(token)
            }
            Err(e) => {
                warn!("Failed to issue submit token: {:#}", e);
                None
            }
        }
    }

    /// Add an inbound address to an instance. Owner-only. `address` is
    /// normalized and classified (exact vs. `*@domain` wildcard) by
    /// `inbound::routing::classify_for_storage`, the same logic `bin/cli.rs`'s
    /// `address add` uses, so the two can never disagree about what counts as
    /// a valid address.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::InstanceOwner(instance_id.to_string()))")]
    async fn add_inbound_address(
        &self,
        instance_id: ID,
        address: String,
    ) -> Result<InboundAddressInfo> {
        let (normalized, kind) =
            routing::classify_for_storage(&address).map_err(ApiError::forbidden)?;
        if let Some(existing) = self.app.db().get_inbound_address(&normalized).await? {
            return Err(ApiError::conflict(format!(
                "{normalized} is already mapped to instance {}",
                existing.instance_id
            ))
            .into());
        }
        let created = self
            .app
            .db()
            .create_inbound_address(&normalized, instance_id.as_str(), kind)
            .await?;
        Ok(created.into())
    }

    /// Remove an inbound address from an instance. Owner-only. An address
    /// that doesn't exist, or that belongs to a different instance, is
    /// reported the same "not found" either way — so this can't be used to
    /// probe another instance's addresses.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::InstanceOwner(instance_id.to_string()))")]
    async fn remove_inbound_address(&self, instance_id: ID, address: String) -> Result<bool> {
        let normalized = address.trim().to_lowercase();
        let existing = self
            .app
            .db()
            .get_inbound_address(&normalized)
            .await?
            .filter(|a| a.instance_id == instance_id.as_str())
            .ok_or_else(|| ApiError::not_found("InboundAddress", &normalized))?;
        self.app
            .db()
            .delete_inbound_address(&existing.address)
            .await?;
        Ok(true)
    }

    // ── Ticket mutations ─────────────────────────────────────────────────────

    /// Open a new ticket from the public submit form. **Requester-token
    /// only** — the instance and the requester's email come from the token
    /// (`AuthInfo::Requester`), never from an argument, so this can't be used
    /// to open a ticket in someone else's name or in an instance the token
    /// wasn't scoped to. The requester's own submission becomes the ticket's
    /// first message, stored as `kind: INBOUND` with `fromEmail` set (never
    /// `authorUserId` — no staff member wrote it).
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Requester)")]
    async fn submit_ticket(
        &self,
        ctx: &Context<'_>,
        subject: String,
        body: String,
    ) -> Result<Ticket<A>> {
        let Some(AuthInfo::Requester { email, instance_id }) = ctx.data_opt::<AuthInfo>() else {
            return Err(ApiError::forbidden("Must hold a requester submit token").into());
        };
        let subject = subject.trim();
        if subject.is_empty() {
            return Err(anyhow!("subject cannot be empty"));
        }
        let body = body.trim();
        if body.is_empty() {
            return Err(anyhow!("body cannot be empty"));
        }

        let instance = self
            .app
            .db()
            .get_instances(&[instance_id])
            .await?
            .into_iter()
            .next()
            .flatten()
            .filter(|i| !i.deleted)
            .ok_or_else(|| ApiError::not_found("Instance", instance_id))?;

        let number = self.app.db().increment_ticket_counter(&instance.id).await?;
        let ticket = self
            .app
            .db()
            .create_ticket(
                &instance.id,
                number,
                subject,
                std::slice::from_ref(email),
                &[],
            )
            .await?;

        self.app
            .db()
            .create_ticket_message(
                &ticket.id,
                db::TicketMessageKind::Inbound,
                None,
                Some(email),
                &[],
                &[],
                Some(body),
                None,
                None,
            )
            .await?;

        info!(
            "Ticket submitted: instance={} ticket={} number={}",
            instance.id, ticket.id, ticket.number
        );
        Ok(Ticket::new(ticket))
    }

    /// Reply to a ticket as a member (agent/owner). Creates the `ticket_message`
    /// row (`kind: REPLY`) and bumps the ticket's activity — **does not
    /// actually send the email**. `TODO(step 6)`: build the outbound MIME
    /// message (`mail-builder`) and send it via SESv2 with the `Reply-To`
    /// threading token; this step only persists the reply and updates the
    /// ticket, which is why `rfc_message_id` is absent on the returned
    /// message (there is no real one yet). Attachments are similarly
    /// `TODO(step 7)` — no `attachmentKeys` argument exists yet.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn reply_to_ticket(
        &self,
        ctx: &Context<'_>,
        ticket_id: ID,
        body: String,
    ) -> Result<TicketMessage<A>> {
        let ticket = require_ticket_member(ctx, &*self.app, ticket_id.as_str()).await?;
        let user_id = require_user_id(ctx)?;
        let body = body.trim();
        if body.is_empty() {
            return Err(anyhow!("body cannot be empty"));
        }

        let msg = self
            .app
            .db()
            .create_ticket_message(
                &ticket.id,
                db::TicketMessageKind::Reply,
                Some(&user_id),
                None,
                &ticket.requester_emails,
                &ticket.cc_emails,
                Some(body),
                None,
                None,
            )
            .await?;

        self.app
            .db()
            .update_ticket(
                &ticket.id,
                db::TicketUpdateShape::Touch {
                    now: crate::clock::now_sec(),
                },
            )
            .await?;

        Ok(TicketMessage::new(msg))
    }

    /// Add an internal note to a ticket. Member-only, same per-ticket
    /// authorization as every other ticket mutation. Stored as `kind: NOTE`,
    /// which `Ticket.messages` filters out for anyone but a member of this
    /// ticket's instance — see that field's doc comment. Never emailed, now
    /// or ever: the outbound-mail path (step 6) only ever sends `REPLY`
    /// messages.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn add_internal_note(
        &self,
        ctx: &Context<'_>,
        ticket_id: ID,
        body: String,
    ) -> Result<TicketMessage<A>> {
        let ticket = require_ticket_member(ctx, &*self.app, ticket_id.as_str()).await?;
        let user_id = require_user_id(ctx)?;
        let body = body.trim();
        if body.is_empty() {
            return Err(anyhow!("body cannot be empty"));
        }

        let msg = self
            .app
            .db()
            .create_ticket_message(
                &ticket.id,
                db::TicketMessageKind::Note,
                Some(&user_id),
                None,
                &[],
                &[],
                Some(body),
                None,
                None,
            )
            .await?;

        self.app
            .db()
            .update_ticket(
                &ticket.id,
                db::TicketUpdateShape::Touch {
                    now: crate::clock::now_sec(),
                },
            )
            .await?;

        Ok(TicketMessage::new(msg))
    }

    /// Change a ticket's status — `OPEN`/`CLOSED`/`DELETED`. Deleting and
    /// restoring a ticket are just this mutation with `DELETED`/`OPEN`: see
    /// `db::TicketUpdateShape::SetStatusAndAssignee`'s doc comment for why
    /// that one variant, not a bespoke delete/restore code path, is what
    /// keeps the three composite marker attributes consistent. A no-op
    /// transition (already at the requested status) still succeeds, returning
    /// the ticket unchanged, rather than erroring — idempotent by design.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn set_ticket_status(
        &self,
        ctx: &Context<'_>,
        ticket_id: ID,
        status: TicketStatusType,
    ) -> Result<Ticket<A>> {
        let ticket = require_ticket_member(ctx, &*self.app, ticket_id.as_str()).await?;
        let new_status: db::TicketStatus = status.into();
        if new_status == ticket.status {
            return Ok(Ticket::new(ticket));
        }
        let now = crate::clock::now_sec();
        self.app
            .db()
            .update_ticket(
                &ticket.id,
                db::TicketUpdateShape::SetStatusAndAssignee {
                    instance_id: &ticket.instance_id,
                    status: new_status,
                    assignee_user_id: ticket.assignee_user_id.as_deref(),
                    now,
                },
            )
            .await?;
        Ok(Ticket::new(db::Ticket {
            status: new_status,
            updated_at: now,
            last_activity_at: now,
            ..ticket
        }))
    }

    /// Assign (or, with `userId: null`, unassign) a ticket. The target user,
    /// when assigning, must themselves be a member of this ticket's
    /// instance — assigning to an outsider would be silently useless (they
    /// could never see it via the "assigned to me" filter, which is itself
    /// instance-scoped).
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn assign_ticket(
        &self,
        ctx: &Context<'_>,
        ticket_id: ID,
        user_id: Option<ID>,
    ) -> Result<Ticket<A>> {
        let ticket = require_ticket_member(ctx, &*self.app, ticket_id.as_str()).await?;
        if let Some(uid) = &user_id {
            let target_memberships = self.app.db().list_memberships_by_user(uid.as_str()).await?;
            if !target_memberships
                .iter()
                .any(|m| m.instance_id == ticket.instance_id)
            {
                return Err(
                    ApiError::forbidden("Assignee must be a member of this instance").into(),
                );
            }
        }
        let now = crate::clock::now_sec();
        let assignee_user_id = user_id.as_ref().map(|id| id.as_str());
        self.app
            .db()
            .update_ticket(
                &ticket.id,
                db::TicketUpdateShape::SetStatusAndAssignee {
                    instance_id: &ticket.instance_id,
                    status: ticket.status,
                    assignee_user_id,
                    now,
                },
            )
            .await?;
        Ok(Ticket::new(db::Ticket {
            assignee_user_id: user_id.map(|id| id.to_string()),
            updated_at: now,
            last_activity_at: now,
            ..ticket
        }))
    }

    /// Add a requester (a "To" recipient) to a ticket.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn add_ticket_requester(
        &self,
        ctx: &Context<'_>,
        ticket_id: ID,
        email: String,
    ) -> Result<Ticket<A>> {
        let ticket = require_ticket_member(ctx, &*self.app, ticket_id.as_str()).await?;
        let email = normalize_ticket_email(&email)?;
        let now = crate::clock::now_sec();
        self.app
            .db()
            .update_ticket(
                &ticket.id,
                db::TicketUpdateShape::AddRequester { email: &email, now },
            )
            .await?;
        let mut requester_emails = ticket.requester_emails.clone();
        if !requester_emails.contains(&email) {
            requester_emails.push(email);
        }
        Ok(Ticket::new(db::Ticket {
            requester_emails,
            updated_at: now,
            last_activity_at: now,
            ..ticket
        }))
    }

    /// Remove a requester from a ticket. Refuses to remove the last one — a
    /// ticket must always have someone to reply to.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn remove_ticket_requester(
        &self,
        ctx: &Context<'_>,
        ticket_id: ID,
        email: String,
    ) -> Result<Ticket<A>> {
        let ticket = require_ticket_member(ctx, &*self.app, ticket_id.as_str()).await?;
        let email = normalize_ticket_email(&email)?;
        if ticket.requester_emails.len() <= 1 && ticket.requester_emails.contains(&email) {
            return Err(ApiError::conflict("A ticket must keep at least one requester").into());
        }
        let now = crate::clock::now_sec();
        self.app
            .db()
            .update_ticket(
                &ticket.id,
                db::TicketUpdateShape::RemoveRequester { email: &email, now },
            )
            .await?;
        let requester_emails = ticket
            .requester_emails
            .iter()
            .filter(|e| **e != email)
            .cloned()
            .collect();
        Ok(Ticket::new(db::Ticket {
            requester_emails,
            updated_at: now,
            last_activity_at: now,
            ..ticket
        }))
    }

    /// Add a CC recipient to a ticket.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn add_ticket_cc(
        &self,
        ctx: &Context<'_>,
        ticket_id: ID,
        email: String,
    ) -> Result<Ticket<A>> {
        let ticket = require_ticket_member(ctx, &*self.app, ticket_id.as_str()).await?;
        let email = normalize_ticket_email(&email)?;
        let now = crate::clock::now_sec();
        self.app
            .db()
            .update_ticket(
                &ticket.id,
                db::TicketUpdateShape::AddCc { email: &email, now },
            )
            .await?;
        let mut cc_emails = ticket.cc_emails.clone();
        if !cc_emails.contains(&email) {
            cc_emails.push(email);
        }
        Ok(Ticket::new(db::Ticket {
            cc_emails,
            updated_at: now,
            last_activity_at: now,
            ..ticket
        }))
    }

    /// Remove a CC recipient from a ticket. Unlike requesters, CCs may go to
    /// zero — there is no "must keep at least one" rule for them.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn remove_ticket_cc(
        &self,
        ctx: &Context<'_>,
        ticket_id: ID,
        email: String,
    ) -> Result<Ticket<A>> {
        let ticket = require_ticket_member(ctx, &*self.app, ticket_id.as_str()).await?;
        let email = normalize_ticket_email(&email)?;
        let now = crate::clock::now_sec();
        self.app
            .db()
            .update_ticket(
                &ticket.id,
                db::TicketUpdateShape::RemoveCc { email: &email, now },
            )
            .await?;
        let cc_emails = ticket
            .cc_emails
            .iter()
            .filter(|e| **e != email)
            .cloned()
            .collect();
        Ok(Ticket::new(db::Ticket {
            cc_emails,
            updated_at: now,
            last_activity_at: now,
            ..ticket
        }))
    }

    /// Revoke the calling token, if there is one to revoke (a `Requester`
    /// capability token, or an impersonated dev-auth session, has none — this is
    /// still a harmless `true` for either).
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn logout(&self, ctx: &Context<'_>) -> Result<bool> {
        if let Some(AuthInfo::User {
            token_id: Some(token_id),
            ..
        }) = ctx.data_opt::<AuthInfo>()
        {
            self.app.db().delete_user_token(token_id).await?;
        }
        Ok(true)
    }

    // ── Passkey (WebAuthn) mutations ─────────────────────────────────────────

    /// Start passkey registration for the authenticated user. Returns a JSON
    /// challenge to pass to the browser's WebAuthn API.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn begin_passkey_registration(&self, ctx: &Context<'_>) -> Result<PasskeyChallenge> {
        use webauthn_rs::prelude::*;

        let user_id = require_user_id(ctx)?;

        let count = self
            .app
            .db()
            .count_webauthn_credentials_by_user(&user_id)
            .await?;
        if count >= MAX_PASSKEYS_PER_USER {
            return Err(anyhow!(
                "Maximum of {MAX_PASSKEYS_PER_USER} passkeys allowed"
            ));
        }

        let existing = self
            .app
            .db()
            .list_webauthn_credentials_by_user(&user_id)
            .await?;

        let webauthn = ctx.data_unchecked::<Arc<Webauthn>>();

        // The user handle stays tied to the (immutable) user id so a passkey
        // keeps working if the user's email changes. Only the display name —
        // what the OS/password manager shows — uses the email.
        let user_uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, user_id.as_bytes());
        let display_name = self
            .app
            .db()
            .get_users(&[&user_id])
            .await?
            .into_iter()
            .next()
            .flatten()
            .map(|u| u.email)
            .unwrap_or_else(|| user_id.clone());

        let existing_cred_ids: Vec<CredentialID> = existing
            .iter()
            .filter_map(|c| {
                serde_json::from_str::<Passkey>(&c.passkey_json)
                    .ok()
                    .map(|pk| pk.cred_id().clone())
            })
            .collect();

        let exclude = if existing_cred_ids.is_empty() {
            None
        } else {
            Some(existing_cred_ids)
        };

        let (ccr, reg_state) = webauthn.start_passkey_registration(
            user_uuid,
            &display_name,
            &display_name,
            exclude,
        )?;

        // Force the credential to be discoverable (a resident key). webauthn-rs's
        // registration options don't expose a builder knob for the modern
        // `residentKey` field (only the legacy `requireResidentKey` boolean), so
        // inject `residentKey: "required"` into the options JSON before handing
        // it to the browser. A platform authenticator makes a credential
        // discoverable by default, but a security key may not unless asked — and
        // a non-discoverable credential can't be used by `beginPasskeyLogin`'s
        // usernameless flow. `finish_passkey_registration` doesn't validate
        // residency itself, so there's no verification mismatch from patching
        // the request but not the response.
        let mut options_value = serde_json::to_value(&ccr.public_key)
            .map_err(|e| anyhow!("Failed to serialize registration options: {e}"))?;
        if let Some(sel) = options_value
            .get_mut("authenticatorSelection")
            .and_then(|v| v.as_object_mut())
        {
            sel.insert("residentKey".to_string(), serde_json::json!("required"));
            sel.insert("requireResidentKey".to_string(), serde_json::json!(true));
        }
        let options_json = serde_json::to_string(&options_value)
            .map_err(|e| anyhow!("Failed to serialize registration options: {e}"))?;
        let state_json = serde_json::to_string(&reg_state)
            .map_err(|e| anyhow!("Failed to serialize registration state: {e}"))?;

        let challenge_id = nanoid::nanoid!(32);
        let expires_at = crate::expire::ExpirePolicy::WebauthnChallenge.from_now();

        self.app
            .db()
            .put_webauthn_state(
                &challenge_id,
                "reg",
                Some(&user_id),
                &state_json,
                expires_at,
            )
            .await?;

        Ok(PasskeyChallenge {
            challenge_id,
            options_json,
        })
    }

    /// Finish passkey registration: verify the browser response and store the
    /// credential.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn finish_passkey_registration(
        &self,
        ctx: &Context<'_>,
        challenge_id: String,
        credential_json: String,
        name: String,
    ) -> Result<PasskeyInfo> {
        use webauthn_rs::prelude::*;

        let user_id = require_user_id(ctx)?;

        let state_record = self
            .app
            .db()
            .get_webauthn_state(&challenge_id)
            .await?
            .ok_or_else(|| anyhow!("Registration challenge not found or expired"))?;

        if state_record.kind != "reg" {
            return Err(anyhow!("Invalid challenge kind"));
        }
        if state_record.user_id.as_deref() != Some(user_id.as_str()) {
            return Err(anyhow!("Challenge belongs to a different user"));
        }

        let now = crate::clock::now_sec();
        if now >= state_record.expires_at {
            let _ = self.app.db().delete_webauthn_state(&challenge_id).await;
            return Err(anyhow!("Registration challenge expired"));
        }

        let reg_state: PasskeyRegistration = serde_json::from_str(&state_record.state_json)
            .map_err(|e| anyhow!("Failed to deserialize registration state: {e}"))?;
        let reg_credential: RegisterPublicKeyCredential = serde_json::from_str(&credential_json)
            .map_err(|e| anyhow!("Failed to parse credential: {e}"))?;

        let webauthn = ctx.data_unchecked::<Arc<Webauthn>>();
        let passkey = webauthn
            .finish_passkey_registration(&reg_credential, &reg_state)
            .map_err(|e| anyhow!("Passkey registration failed: {e}"))?;

        // Re-check the cap to guard against a race with a second concurrent
        // registration from the same user.
        let count = self
            .app
            .db()
            .count_webauthn_credentials_by_user(&user_id)
            .await?;
        if count >= MAX_PASSKEYS_PER_USER {
            let _ = self.app.db().delete_webauthn_state(&challenge_id).await;
            return Err(anyhow!(
                "Maximum of {MAX_PASSKEYS_PER_USER} passkeys allowed"
            ));
        }

        let cred_id = URL_SAFE_NO_PAD.encode(passkey.cred_id().as_ref());
        let passkey_json = serde_json::to_string(&passkey)
            .map_err(|e| anyhow!("Failed to serialize passkey: {e}"))?;

        let cred = self
            .app
            .db()
            .create_webauthn_credential(&cred_id, &user_id, &name, &passkey_json)
            .await?;

        let _ = self.app.db().delete_webauthn_state(&challenge_id).await;

        info!(
            "Passkey registered for user_id={} cred_id={}",
            user_id, cred_id
        );

        Ok(cred.into())
    }

    /// Start a discoverable passkey login (no username required). Returns a JSON
    /// challenge to pass to the browser's WebAuthn API.
    async fn begin_passkey_login(&self, ctx: &Context<'_>) -> Result<PasskeyChallenge> {
        use webauthn_rs::prelude::*;

        let webauthn = ctx.data_unchecked::<Arc<Webauthn>>();
        let (rcr, auth_state) = webauthn
            .start_discoverable_authentication()
            .map_err(|e| anyhow!("Failed to start passkey login: {e}"))?;

        let options_json = serde_json::to_string(&rcr.public_key)
            .map_err(|e| anyhow!("Failed to serialize login options: {e}"))?;
        let state_json = serde_json::to_string(&auth_state)
            .map_err(|e| anyhow!("Failed to serialize auth state: {e}"))?;

        let challenge_id = nanoid::nanoid!(32);
        let expires_at = crate::expire::ExpirePolicy::WebauthnChallenge.from_now();

        self.app
            .db()
            .put_webauthn_state(&challenge_id, "auth", None, &state_json, expires_at)
            .await?;

        Ok(PasskeyChallenge {
            challenge_id,
            options_json,
        })
    }

    /// Finish passkey login: verify the browser response and return an opaque
    /// `mtu_` token. `Ok(None)` for anything that means "this credential doesn't
    /// authenticate" (expired challenge, unknown credential, disabled user) — an
    /// `Err` is reserved for the request itself being malformed.
    async fn finish_passkey_login(
        &self,
        ctx: &Context<'_>,
        challenge_id: String,
        credential_json: String,
    ) -> Result<Option<String>> {
        use webauthn_rs::prelude::*;

        let state_record = self
            .app
            .db()
            .get_webauthn_state(&challenge_id)
            .await?
            .ok_or_else(|| anyhow!("Login challenge not found or expired"))?;

        if state_record.kind != "auth" {
            return Err(anyhow!("Invalid challenge kind"));
        }

        let now = crate::clock::now_sec();
        if now >= state_record.expires_at {
            let _ = self.app.db().delete_webauthn_state(&challenge_id).await;
            return Ok(None);
        }

        let auth_state: DiscoverableAuthentication = serde_json::from_str(&state_record.state_json)
            .map_err(|e| anyhow!("Failed to deserialize auth state: {e}"))?;
        let auth_credential: PublicKeyCredential = serde_json::from_str(&credential_json)
            .map_err(|_| anyhow!("Failed to parse credential"))?;

        let webauthn = ctx.data_unchecked::<Arc<Webauthn>>();
        let (_user_handle, cred_id_bytes) = webauthn
            .identify_discoverable_authentication(&auth_credential)
            .map_err(|e| anyhow!("Failed to identify credential: {e}"))?;

        let cred_id_str = URL_SAFE_NO_PAD.encode(cred_id_bytes);
        let stored = match self.app.db().get_webauthn_credential(&cred_id_str).await? {
            Some(c) => c,
            None => {
                info!("finish_passkey_login: unknown credential {}", cred_id_str);
                let _ = self.app.db().delete_webauthn_state(&challenge_id).await;
                return Ok(None);
            }
        };

        let mut passkey: Passkey = serde_json::from_str(&stored.passkey_json)
            .map_err(|e| anyhow!("Failed to deserialize stored passkey: {e}"))?;

        let auth_result = webauthn
            .finish_discoverable_authentication(
                &auth_credential,
                auth_state,
                &[DiscoverableKey::from(&passkey)],
            )
            .map_err(|e| anyhow!("Passkey authentication failed: {e}"))?;

        // Always record last_used_at on a successful login. The counter bump is
        // conditional (needs_update() only fires when the signature counter
        // advanced), but most platform/synced passkeys keep the counter at 0 and
        // never report needs_update(), so gating the whole write on it would
        // leave last_used_at perpetually unset for the common case.
        if auth_result.needs_update() {
            passkey.update_credential(&auth_result);
        }
        let updated_json = serde_json::to_string(&passkey)
            .map_err(|e| anyhow!("Failed to serialize updated passkey: {e}"))?;
        let _ = self
            .app
            .db()
            .update_webauthn_credential(
                &cred_id_str,
                db::WebauthnCredentialUpdate::TouchLastUsed {
                    passkey_json: updated_json,
                },
            )
            .await;

        let _ = self.app.db().delete_webauthn_state(&challenge_id).await;

        match self
            .app
            .db()
            .get_users(&[&stored.user_id])
            .await?
            .into_iter()
            .next()
            .flatten()
        {
            Some(user) if user.enabled => {}
            _ => {
                info!(
                    "finish_passkey_login: user disabled or missing id={}",
                    stored.user_id
                );
                return Ok(None);
            }
        }

        let token = auth::issue_user_token(&*self.app, &stored.user_id).await?;
        info!(
            "Passkey login for user_id={} cred_id={}",
            stored.user_id, cred_id_str
        );
        Ok(Some(token))
    }

    /// Rename one of the authenticated user's passkeys. A credential belonging
    /// to someone else reports the same "not found" as a nonexistent one — never
    /// "belongs to another user" — so this can't be used to probe who owns a
    /// guessed credential id.
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn rename_passkey(
        &self,
        ctx: &Context<'_>,
        id: String,
        name: String,
    ) -> Result<PasskeyInfo> {
        let user_id = require_user_id(ctx)?;

        let cred = self
            .app
            .db()
            .get_webauthn_credential(&id)
            .await?
            .ok_or_else(|| anyhow!("Passkey not found"))?;
        if cred.user_id != user_id {
            return Err(anyhow!("Passkey not found"));
        }

        let trimmed = name.trim().to_string();
        if trimmed.is_empty() {
            return Err(anyhow!("Name cannot be empty"));
        }

        self.app
            .db()
            .update_webauthn_credential(&id, db::WebauthnCredentialUpdate::Rename(trimmed.clone()))
            .await?;

        Ok(PasskeyInfo {
            id: cred.id,
            name: trimmed,
            created_at: cred.created_at as i64,
            last_used_at: cred.last_used_at.map(|t| t as i64),
        })
    }

    /// Delete one of the authenticated user's passkeys. Same ownership-hiding
    /// "not found" as [`Self::rename_passkey`].
    #[graphql(guard = "AuthGuard::new(AuthRequirement::Authenticated)")]
    async fn delete_passkey(&self, ctx: &Context<'_>, id: String) -> Result<bool> {
        let user_id = require_user_id(ctx)?;

        let cred = self
            .app
            .db()
            .get_webauthn_credential(&id)
            .await?
            .ok_or_else(|| anyhow!("Passkey not found"))?;
        if cred.user_id != user_id {
            return Err(anyhow!("Passkey not found"));
        }

        self.app.db().delete_webauthn_credential(&id).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanitized fixture, hand-constructed (not captured from a real device) to
    /// match `webauthn-rs` 0.5.5's `Passkey` serde shape exactly — every field
    /// name and enum variant string below was read straight out of that crate's
    /// source (`interface.rs`, `webauthn-rs-proto`), not guessed. Key bytes are
    /// zeroed. This test exists to catch a `webauthn-rs` serde format change
    /// during a future library upgrade — if deserialization breaks here, stored
    /// passkeys in DynamoDB are at risk of the same break.
    const PASSKEY_JSON_V0_5: &str = r#"{"cred":{"cred_id":"AAAAAAAAAAAAAAAAAAAAAAAAAAAA","cred":{"type_":"ES256","key":{"EC_EC2":{"curve":"SECP256R1","x":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","y":"BAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}}},"counter":0,"transports":null,"user_verified":true,"backup_eligible":true,"backup_state":true,"registration_policy":"preferred","extensions":{"cred_protect":"NotRequested","hmac_create_secret":"NotRequested","appid":"NotRequested","cred_props":"Ignored"},"attestation":{"data":"None","metadata":"None"},"attestation_format":"none"}}"#;

    #[test]
    fn passkey_json_round_trips() {
        use webauthn_rs::prelude::Passkey;
        let passkey: Passkey = serde_json::from_str(PASSKEY_JSON_V0_5).expect(
            "stored passkey JSON must deserialize — format changed after a webauthn-rs upgrade?",
        );
        let reserialized =
            serde_json::to_string(&passkey).expect("passkey must reserialize to JSON");
        let reparsed: Passkey =
            serde_json::from_str(&reserialized).expect("reserialized passkey must round-trip");
        let rereserialized =
            serde_json::to_string(&reparsed).expect("reparsed passkey must reserialize");
        assert_eq!(
            reserialized, rereserialized,
            "passkey JSON must be stable across serde round trips"
        );
    }

    #[test]
    fn sha256_hex_is_stable_and_matches_length() {
        let h = sha256_hex("123456");
        assert_eq!(h, sha256_hex("123456"));
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(h, sha256_hex("654321"));
    }
}
