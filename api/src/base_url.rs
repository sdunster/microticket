//! Public base URLs the API needs to build absolute links: the web app's origin
//! (the OAuth consent page) and the API's own origin (OAuth issuer/endpoint
//! metadata, the MCP resource URL).

/// The web app's origin. This is [`crate::staff_notify::app_base_url`] — the same
/// `APP_BASE_URL` the staff-notification links use — re-exported under the name
/// the OAuth code reads better with, so there is one variable, not two, for
/// "where does the web app live".
pub fn web_base_url() -> String {
    crate::staff_notify::app_base_url()
}

/// Env var holding an explicit API origin, for when the request's `Host` header
/// isn't trustworthy or convenient to derive from (e.g. behind a CDN).
const API_BASE_URL_VAR: &str = "API_BASE_URL";

/// The API's own public origin, used as the OAuth issuer and to build the
/// `token_endpoint`/`registration_endpoint` metadata URLs and the MCP resource.
///
/// Prefers the explicit `API_BASE_URL` env var — **required behind CloudFront**,
/// which does not forward `Host`: without it the fallback below would name the
/// Function URL's host, and OAuth clients would reject the issuer mismatch.
/// Falling back to the request's `Host` header is deliberately last-resort and
/// derived, not configured: it is only correct when the API is reached at its own
/// origin (the raw Function URL, or `poem-local`'s `localhost:8000`). The scheme
/// is `http` only for `localhost`/`127.0.0.1` hosts (local dev); everything else
/// is `https`.
pub fn api_base_url(host_header: Option<&str>) -> String {
    if let Ok(base) = std::env::var(API_BASE_URL_VAR) {
        let base = base.trim();
        if !base.is_empty() {
            return base.trim_end_matches('/').to_string();
        }
    }
    let host = host_header.unwrap_or("localhost:8000").trim();
    let scheme = if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
        "http"
    } else {
        "https"
    };
    format!("{scheme}://{host}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env var tests must not run concurrently with each other (or with anything
    // else touching these vars) — serialize them behind a single mutex. Every
    // test here is synchronous, so a std mutex is never held across an `.await`
    // (see CLAUDE.md's house rule for async tests that touch the environment).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env<T>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|(k, _)| (k.to_string(), std::env::var(k).ok()))
            .collect();
        // SAFETY: serialized by `ENV_LOCK`.
        unsafe {
            for (k, v) in vars {
                match v {
                    Some(v) => std::env::set_var(k, v),
                    None => std::env::remove_var(k),
                }
            }
        }
        let result = f();
        unsafe {
            for (k, v) in previous {
                match v {
                    Some(v) => std::env::set_var(&k, v),
                    None => std::env::remove_var(&k),
                }
            }
        }
        result
    }

    #[test]
    fn api_base_url_prefers_the_explicit_var() {
        with_env(
            &[("API_BASE_URL", Some("https://toolbox.example/"))],
            || {
                assert_eq!(
                    api_base_url(Some("ignored.example.com")),
                    "https://toolbox.example"
                )
            },
        );
    }

    #[test]
    fn api_base_url_derives_https_from_host_by_default() {
        with_env(&[("API_BASE_URL", None)], || {
            assert_eq!(
                api_base_url(Some("abc123.lambda-url.ap-southeast-2.on.aws")),
                "https://abc123.lambda-url.ap-southeast-2.on.aws"
            )
        });
    }

    #[test]
    fn api_base_url_uses_http_for_localhost_and_loopback() {
        with_env(&[("API_BASE_URL", None)], || {
            assert_eq!(
                api_base_url(Some("localhost:8000")),
                "http://localhost:8000"
            );
            assert_eq!(
                api_base_url(Some("127.0.0.1:8000")),
                "http://127.0.0.1:8000"
            );
        });
    }

    #[test]
    fn api_base_url_falls_back_to_localhost_with_no_host() {
        with_env(&[("API_BASE_URL", None)], || {
            assert_eq!(api_base_url(None), "http://localhost:8000")
        });
    }
}
