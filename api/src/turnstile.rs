//! Cloudflare Turnstile CAPTCHA verification for `requestAuthCode`.
//!
//! Ported from seslogin's `turnstile.rs`, with one deliberate change: verification
//! here is skipped whenever `TURNSTILE_SECRET_KEY` is unset, not only when an
//! explicit `TURNSTILE_DISABLED=1` is set. That's the build plan's "Turnstile is
//! optional" assumption — a fresh fork with no Cloudflare account must work out of
//! the box, and requiring an explicit opt-out env var in addition to just not
//! having a secret key would defeat that. `TURNSTILE_DISABLED=1` (as `.env` ships
//! by default) remains a supported override for a fork that *has* configured a
//! secret key but wants to skip verification anyway (e.g. for screenshot/e2e
//! runs) — matching seslogin's variable name so the two projects stay easy to
//! cross-reference.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

#[derive(Deserialize)]
struct SiteverifyResponse {
    success: bool,
    /// Machine-readable failure reasons (e.g. `invalid-input-secret`,
    /// `timeout-or-duplicate`, `missing-input-response`). Present only on failure.
    #[serde(default, rename = "error-codes")]
    error_codes: Vec<String>,
    /// Hostname the challenge was solved on, per Cloudflare.
    #[serde(default)]
    hostname: Option<String>,
}

/// Whether Turnstile verification is actually going to run. `false` when
/// `TURNSTILE_DISABLED=1` is set, or when `TURNSTILE_SECRET_KEY` is unset/blank.
pub fn enabled() -> bool {
    if std::env::var("TURNSTILE_DISABLED").as_deref() == Ok("1") {
        return false;
    }
    std::env::var("TURNSTILE_SECRET_KEY").is_ok_and(|v| !v.trim().is_empty())
}

/// Log Turnstile's effective state once, at process startup. Called by every
/// binary that can reach `verify` (the poem servers; deliberately *not* wired
/// into a CLI flag the way `--dev-auth-user` is, since there is nothing to
/// configure here beyond the env vars `verify` itself reads) so an operator
/// running a fork sees immediately whether `requestAuthCode` is actually
/// CAPTCHA-protected, rather than discovering it the first time a code is
/// requested.
pub fn log_startup_state() {
    if enabled() {
        tracing::info!("Turnstile verification enabled");
    } else {
        tracing::warn!(
            "Turnstile verification disabled (TURNSTILE_SECRET_KEY unset or TURNSTILE_DISABLED=1) \
             — requestAuthCode has no CAPTCHA protection"
        );
    }
}

/// Verify a Cloudflare Turnstile token server-side.
///
/// `remote_ip` is the client's IP, passed through to Cloudflare as `remoteip` to
/// improve its scoring; pass `None` when it isn't available.
///
/// Returns `Ok(true)` immediately, with no network call, when [`enabled`] is
/// `false` — see the module doc. When enabled, `token: None` (the caller omitted
/// `turnstileToken` entirely) is treated as a failed challenge, not skipped
/// verification: a real client always supplies a token once Turnstile is
/// configured. Returns `Ok(false)` on a completed-but-failed challenge; `Err` for
/// configuration issues or network failures.
pub async fn verify(token: Option<&str>, remote_ip: Option<&str>) -> Result<bool> {
    if !enabled() {
        return Ok(true);
    }
    let Some(token) = token else {
        return Ok(false);
    };

    let secret = std::env::var("TURNSTILE_SECRET_KEY")
        .context("TURNSTILE_SECRET_KEY not set despite Turnstile being enabled")?;

    let mut form = vec![("secret", secret.as_str()), ("response", token)];
    if let Some(ip) = remote_ip {
        form.push(("remoteip", ip));
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(SITEVERIFY_URL)
        .form(&form)
        .send()
        .await
        .context("Failed to reach Turnstile siteverify endpoint")?;

    if !resp.status().is_success() {
        bail!(
            "Turnstile siteverify returned HTTP {}",
            resp.status().as_u16()
        );
    }

    let body: SiteverifyResponse = resp
        .json()
        .await
        .context("Failed to parse Turnstile siteverify response")?;

    if !body.success {
        tracing::warn!(
            error_codes = ?body.error_codes,
            hostname = ?body.hostname,
            "Turnstile siteverify rejected token"
        );
    }

    Ok(body.success)
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests share process-global env vars, so they must not run
    // concurrently with each other (`cargo test` runs tests in the same process
    // on multiple threads by default).
    /// `tokio::sync::Mutex`, not `std::sync::Mutex`, for the same reason
    /// `mail::OVERRIDE_TO_ENV_LOCK` is one: the async tests below must hold the
    /// guard **across** their `.await`, which a std guard must never do (see
    /// `clippy::await_holding_lock`) but a tokio guard is built for.
    ///
    /// Releasing the lock before the await — which is what this used to do, to
    /// satisfy that lint — left a window in which a parallel test could set
    /// `TURNSTILE_SECRET_KEY` between the reset and `verify` reading it, so
    /// `enabled()` came back true and the assertion failed. It passed locally
    /// and failed in CI, which is the usual way an env-var race announces
    /// itself. `blocking_lock()` is safe in the synchronous `#[test]`s here
    /// because none of them run inside a tokio runtime.
    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn with_env<T>(disabled: Option<&str>, secret: Option<&str>, f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.blocking_lock();
        // SAFETY: serialized by ENV_LOCK; this test binary doesn't read these
        // vars concurrently elsewhere.
        unsafe {
            std::env::remove_var("TURNSTILE_DISABLED");
            std::env::remove_var("TURNSTILE_SECRET_KEY");
            if let Some(v) = disabled {
                std::env::set_var("TURNSTILE_DISABLED", v);
            }
            if let Some(v) = secret {
                std::env::set_var("TURNSTILE_SECRET_KEY", v);
            }
        }
        let result = f();
        unsafe {
            std::env::remove_var("TURNSTILE_DISABLED");
            std::env::remove_var("TURNSTILE_SECRET_KEY");
        }
        result
    }

    #[test]
    fn disabled_when_secret_key_is_unset() {
        with_env(None, None, || {
            assert!(!enabled());
        });
    }

    #[test]
    fn disabled_when_secret_key_is_blank() {
        with_env(None, Some("   "), || {
            assert!(!enabled());
        });
    }

    #[test]
    fn enabled_when_secret_key_is_set() {
        with_env(None, Some("secret"), || {
            assert!(enabled());
        });
    }

    #[test]
    fn disabled_flag_overrides_a_configured_secret() {
        with_env(Some("1"), Some("secret"), || {
            assert!(!enabled());
        });
    }

    #[tokio::test]
    async fn verify_skips_the_network_when_disabled() {
        // The guard is held across the `.await` deliberately. Releasing it
        // after the env mutation leaves a window in which another test can set
        // TURNSTILE_SECRET_KEY before `verify` reads it.
        let _guard = ENV_LOCK.lock().await;
        // SAFETY: serialized by ENV_LOCK.
        unsafe {
            std::env::remove_var("TURNSTILE_DISABLED");
            std::env::remove_var("TURNSTILE_SECRET_KEY");
        }
        let result = verify(None, None).await;
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn verify_fails_a_missing_token_when_enabled() {
        // Enabled but no token supplied — must fail closed, not skip
        // verification just because the argument was omitted. `verify` returns
        // before any network access when `token` is `None`, so this never
        // depends on TURNSTILE_SECRET_KEY still being set once it's read.
        let _guard = ENV_LOCK.lock().await;
        // SAFETY: serialized by ENV_LOCK, held across the await below.
        unsafe {
            std::env::remove_var("TURNSTILE_DISABLED");
            std::env::set_var("TURNSTILE_SECRET_KEY", "secret");
        }
        let result = verify(None, None).await;
        // SAFETY: still holding ENV_LOCK.
        unsafe {
            std::env::remove_var("TURNSTILE_SECRET_KEY");
        }
        assert!(!result.unwrap());
    }
}
