//! The GraphQL API server, against real AWS: DynamoDB and SES.
//!
//! For an AWS-free local run, see `poem-local.rs` — a separate binary rather than a
//! flag on this one, so this server has no code path that can be talked into
//! mocking anything.

use std::error::Error;

use microticket::dynamodb;
use microticket::s3storage;
use microticket::server;
use microticket::sesmail;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let startup = server::init()?;
    let db = dynamodb::Handler::new(&startup.db_prefix, !startup.cli.enable_mutations).await;
    let mailer = sesmail::Mailer::new().await;
    let storage = s3storage::Storage::new().await?;
    server::run(startup, db, mailer, storage).await
}
