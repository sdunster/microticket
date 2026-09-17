//! The GraphQL API server with email mocked out: mail is logged rather than sent.
//! Started by `make dev-local`; see DEVELOPMENT.md, "Running without AWS".
//!
//! Deliberately a separate binary from `poem.rs`, not a flag or a cargo feature on
//! it. A feature would be switched on by `--all-features`, which `make check`
//! passes to clippy — the real SES path would then stop being linted, and any
//! `--all-features` build would quietly produce a mocked server.
//!
//! DynamoDB is *not* mocked: `DB_PREFIX` and `AWS_ENDPOINT_URL_DYNAMODB` still
//! decide which database this talks to, so it can be pointed at DynamoDB Local or
//! at a real table. That combination — a real database that provably cannot send
//! email — is the point.

use std::error::Error;

use microticket::dynamodb;
use microticket::mockmail;
use microticket::server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let startup = server::init()?;
    tracing::warn!("poem-local: SES is mocked. Email will be logged instead of sent.");
    let db = dynamodb::Handler::new(&startup.db_prefix, !startup.cli.enable_mutations).await;
    server::run(startup, db, mockmail::Handler::from_env()).await
}
