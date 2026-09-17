//! The GraphQL API Lambda (`api-lambda`), served over a Function URL.
//!
//! Schema and app are built once here, outside the handler closure, so a warm
//! Lambda instance reuses them across invocations instead of rebuilding the schema
//! (and re-resolving WebAuthn config) on every request.

mod errors;
mod handler;

use lambda_http::{Error, run, service_fn, tracing};
use microticket::app;
use microticket::dynamodb;
use microticket::graphql;
use microticket::sesmail;
use std::env;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    // JWT_SECRET isn't consumed by anything yet — microticket has no JWTs, only
    // opaque `mtu_` tokens, and token verification itself is step 3's job. Reading
    // the real auth secret from SSM at cold start is step 9's concern; for now this
    // just warns rather than requiring the env var to be set at all.
    if env::var("JWT_SECRET").is_err() {
        tracing::warn!(
            "JWT_SECRET is not set. Token verification isn't implemented until step 3, but a \
             value will be required once it is."
        );
    }

    let mailer = sesmail::Mailer::new().await;

    let read_only = env::var("READ_ONLY")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let db_prefix = env::var("DB_PREFIX").expect("DB_PREFIX must be set for dynamodb backend");
    let db = dynamodb::Handler::new(&db_prefix, read_only).await;
    let app = Arc::new(app::new(db, mailer, 0));
    let webauthn = Arc::new(app::build_webauthn().expect("WebAuthn build failed"));
    let schema = graphql::build_schema(app.clone(), webauthn);
    let handler = handler::Handler::new(app, schema);
    run(service_fn(|req| handler.handle_request(req))).await
}
