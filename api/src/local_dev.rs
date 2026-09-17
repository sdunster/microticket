//! Guards shared by the local-development binaries (`local-tables`, `local-seed`,
//! once `local/` grows them in a later step).
//!
//! These binaries create tables and write rows unconditionally, which is only ever
//! safe against a DynamoDB running on this machine. The check lives here rather
//! than in each binary so the two cannot drift apart.

use anyhow::{Result, anyhow, bail};

/// The configured DynamoDB endpoint, but only if it is unmistakably a local one.
///
/// This is the whole safety story for the local-dev binaries: with no endpoint
/// override the AWS SDK talks to real DynamoDB using whatever credentials are lying
/// around, and a stray `DB_PREFIX` would then create or overwrite rows in a real
/// account.
pub fn require_local_dynamodb_endpoint() -> Result<String> {
    let endpoint = std::env::var("AWS_ENDPOINT_URL_DYNAMODB")
        .or_else(|_| std::env::var("AWS_ENDPOINT_URL"))
        .map_err(|_| {
            anyhow!(
                "AWS_ENDPOINT_URL_DYNAMODB is not set — refusing to run in case this would \
                 write to a real AWS account. Use the `make local-*` targets, which point it \
                 at the DynamoDB Local started by local/dynamodb.sh."
            )
        })?;
    let url = url::Url::parse(&endpoint)
        .map_err(|e| anyhow!("AWS_ENDPOINT_URL_DYNAMODB is not a URL: {e}"))?;
    match url.host_str() {
        Some("localhost" | "127.0.0.1" | "::1" | "[::1]") => Ok(endpoint),
        other => bail!(
            "DynamoDB endpoint {:?} is not local (host {:?}) — refusing to run.",
            endpoint,
            other.unwrap_or("<none>")
        ),
    }
}

/// A DynamoDB client for the given endpoint-and-region environment.
pub async fn dynamodb_client() -> aws_sdk_dynamodb::Client {
    let config = crate::aws_config_loader()
        .region(aws_config::Region::new(
            std::env::var("AWS_REGION").unwrap_or_else(|_| "ap-southeast-2".to_string()),
        ))
        .load()
        .await;
    aws_sdk_dynamodb::Client::new(&config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_endpoint<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized by ENV_LOCK; this test binary doesn't read these vars
        // concurrently elsewhere.
        unsafe {
            std::env::remove_var("AWS_ENDPOINT_URL_DYNAMODB");
            std::env::remove_var("AWS_ENDPOINT_URL");
            if let Some(v) = value {
                std::env::set_var("AWS_ENDPOINT_URL_DYNAMODB", v);
            }
        }
        let result = f();
        unsafe {
            std::env::remove_var("AWS_ENDPOINT_URL_DYNAMODB");
        }
        result
    }

    #[test]
    fn refuses_when_unset() {
        with_endpoint(None, || {
            assert!(require_local_dynamodb_endpoint().is_err());
        });
    }

    #[test]
    fn accepts_localhost() {
        with_endpoint(Some("http://localhost:8100"), || {
            assert_eq!(
                require_local_dynamodb_endpoint().unwrap(),
                "http://localhost:8100"
            );
        });
    }

    #[test]
    fn accepts_loopback_ip() {
        with_endpoint(Some("http://127.0.0.1:8100"), || {
            assert!(require_local_dynamodb_endpoint().is_ok());
        });
    }

    #[test]
    fn refuses_a_real_endpoint() {
        with_endpoint(
            Some("https://dynamodb.ap-southeast-2.amazonaws.com"),
            || {
                assert!(require_local_dynamodb_endpoint().is_err());
            },
        );
    }
}
