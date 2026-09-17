//! Outgoing-email abstraction.
//!
//! Mirrors the [`crate::db`] / [`crate::dynamodb`] / [`crate::mockdb`] split: this
//! module holds the trait, [`crate::sesmail`] sends via AWS SES, and
//! [`crate::mockmail`] logs the message instead, so the API can run — including the
//! email-code login flow (`requestAuthCode`) — with no AWS account.
//!
//! Only `send_plain_text` and `send_html` for now. `send_raw` and MIME/attachment
//! building (needed for ticket replies with real threading headers) are step 6's
//! job — see `sesmail.rs`'s doc comment.

use anyhow::Result;
use std::future::Future;

/// Placeholder sender/reply-to addresses, used until step 6 wires up per-instance
/// addressing (`"{instance.from_name}" <{instance's inbound address}>`, with
/// `Reply-To` carrying the ticket's reply token). Only exercised by the code that
/// exists so far (nothing, yet) and by tests.
pub const FROM: &str = "no-reply@microticket.test";
pub const REPLY_TO: &str = "support@microticket.test";

/// Redirect every outgoing email to this address instead of its real recipient.
///
/// A fork's local dev environment may be pointed at a database that carries real
/// member/requester email addresses (e.g. a sanitized snapshot of prod). Setting
/// `MAIL_OVERRIDE_TO` in `.env` makes it impossible for a local run to mail a real
/// person. Never set it in a deployed environment.
const OVERRIDE_TO_VAR: &str = "MAIL_OVERRIDE_TO";

/// Serializes every test in this crate that reads or writes the process-global
/// `MAIL_OVERRIDE_TO` env var — not just the ones in this module's own `tests`
/// submodule. `cargo test` runs tests from every module in one process on
/// multiple threads, so `mockmail::tests::records_what_was_sent` (which asserts
/// on `resolve_recipient`'s *unset* behaviour) must serialize against this
/// module's own tests (which set and clear the var) or it can observe another
/// thread's in-flight override and fail nondeterministically.
///
/// `tokio::sync::Mutex`, not `std::sync::Mutex`: `mockmail`'s test is
/// `#[tokio::test]` and needs to hold the guard across an `.await`, which is
/// exactly what a `std`/`parking_lot` guard must never do (see
/// `clippy::await_holding_lock`) but a tokio guard is designed for.
/// `blocking_lock()` (used by this module's own, synchronous `#[test]`s) is
/// safe here specifically because none of those tests run inside a tokio
/// runtime themselves.
#[cfg(test)]
pub(crate) static OVERRIDE_TO_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Resolve the address to actually send to, honouring [`OVERRIDE_TO_VAR`].
///
/// Applied by every backend, mock included, so what the mock logs is what a real
/// send would have done.
pub fn resolve_recipient(to: &str) -> String {
    match std::env::var(OVERRIDE_TO_VAR) {
        Ok(override_to) if !override_to.trim().is_empty() => {
            let override_to = override_to.trim().to_string();
            tracing::warn!("{OVERRIDE_TO_VAR} is set: redirecting email for {to} to {override_to}");
            override_to
        }
        _ => to.to_string(),
    }
}

/// `Sync` is required for the same reason as [`crate::db::Handler`]: a
/// `&impl Handler` is held across `.await` inside the `Send` futures the
/// GraphQL/Poem stack builds.
pub trait Handler: Sync {
    fn send_plain_text(
        &self,
        to: &str,
        subject: &str,
        content: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    fn send_html(
        &self,
        to: &str,
        subject: &str,
        html: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests share the process-global `MAIL_OVERRIDE_TO` env var (as does
    // `mockmail::tests::records_what_was_sent`), so they must not run
    // concurrently with anything else that touches it — see
    // `OVERRIDE_TO_ENV_LOCK`'s doc comment.
    use super::OVERRIDE_TO_ENV_LOCK as ENV_LOCK;

    #[test]
    fn resolve_recipient_passes_through_when_unset() {
        let _guard = ENV_LOCK.blocking_lock();
        // SAFETY: serialized by ENV_LOCK; no other thread reads/writes this var
        // concurrently within this test binary.
        unsafe {
            std::env::remove_var(OVERRIDE_TO_VAR);
        }
        assert_eq!(resolve_recipient("a@example.com"), "a@example.com");
    }

    #[test]
    fn resolve_recipient_redirects_when_set() {
        let _guard = ENV_LOCK.blocking_lock();
        // SAFETY: serialized by ENV_LOCK.
        unsafe {
            std::env::set_var(OVERRIDE_TO_VAR, " override@example.com ");
        }
        assert_eq!(resolve_recipient("a@example.com"), "override@example.com");
        unsafe {
            std::env::remove_var(OVERRIDE_TO_VAR);
        }
    }

    #[test]
    fn resolve_recipient_ignores_a_blank_override() {
        let _guard = ENV_LOCK.blocking_lock();
        // SAFETY: serialized by ENV_LOCK.
        unsafe {
            std::env::set_var(OVERRIDE_TO_VAR, "   ");
        }
        assert_eq!(resolve_recipient("a@example.com"), "a@example.com");
        unsafe {
            std::env::remove_var(OVERRIDE_TO_VAR);
        }
    }
}
