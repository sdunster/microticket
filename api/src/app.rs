//! Dependency-injection container: which concrete backends a binary compiles in.
//!
//! Every backend is a type parameter, so each binary compiles exactly the
//! implementations it uses: `bin/poem.rs` gets DynamoDB + SES, `bin/poem-local.rs`
//! gets DynamoDB + the mock mailer, and `bin/lambda` gets DynamoDB + SES. There is
//! deliberately no runtime switch — a server that could be talked into mocking its
//! own email by an environment variable is a worse thing to deploy than two
//! binaries. A cargo feature was considered and rejected for the same reason:
//! `make check` runs `cargo clippy --all-targets --all-features`, so a mock-mail
//! feature would be silently compiled into every lint pass (and could just as
//! easily be turned on in a real build); a second binary can't be turned on by
//! accident.
//!
//! **Deliberate simplification vs seslogin: there is no queue abstraction.**
//! seslogin has `HasQueues`/`queue.rs`/`sqs.rs`/`mockqueue.rs` because its API
//! *produces* to SQS (member sync, NITC export, healthcheck pings). microticket's
//! API never produces to SQS — the only queue in this system carries inbound mail,
//! and that queue is *consumed* by the inbound-mail Lambda (step 7), which is a
//! separate binary with no GraphQL surface. So there is nothing here to abstract;
//! this decision is also recorded in `CLAUDE.md`.

use crate::db;
use crate::mail;
use crate::storage;
use webauthn_rs::prelude::{Webauthn, WebauthnBuilder};

pub trait App {
    /// Artificial delay (in ms) injected before handling every request. Dev-only,
    /// set by `--response-lag-ms` on `bin/poem`/`bin/poem-local`; always `0` on the
    /// Lambda binary.
    fn response_lag(&self) -> u64;
}

/// Build the WebAuthn relying-party instance from environment configuration.
///
/// `WEBAUTHN_RP_ID` is the relying-party ID (defaults to `localhost`).
/// `WEBAUTHN_RP_ORIGIN` is a comma-separated list of allowed origins; the first is
/// the primary (defaults to `http://localhost:5173`). Multiple origins let a single
/// deployment serve more than one frontend origin.
pub fn build_webauthn() -> anyhow::Result<Webauthn> {
    let rp_id = std::env::var("WEBAUTHN_RP_ID").unwrap_or_else(|_| "localhost".to_string());
    let origins_raw =
        std::env::var("WEBAUTHN_RP_ORIGIN").unwrap_or_else(|_| "http://localhost:5173".to_string());
    let mut origins = origins_raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(url::Url::parse)
        .collect::<Result<Vec<_>, _>>()?;
    if origins.is_empty() {
        return Err(anyhow::anyhow!(
            "WEBAUTHN_RP_ORIGIN must contain at least one origin"
        ));
    }
    let primary = origins.remove(0);
    let mut builder = WebauthnBuilder::new(&rp_id, &primary)?.rp_name("microticket");
    for extra in &origins {
        builder = builder.append_allowed_origin(extra);
    }
    Ok(builder.build()?)
}

pub trait HasDb {
    fn db(&self) -> &impl db::Handler;
}

pub trait HasMail {
    fn mail(&self) -> &impl mail::Handler;
}

/// Object storage (the mail bucket: raw MIME, attachments, presigned
/// upload/download) — step 7's addition alongside `HasDb`/`HasMail`.
pub trait HasStorage {
    fn storage(&self) -> &impl storage::Handler;
}

/// Struct holding our global singletons. See the module doc for why every backend
/// is a type parameter and there is no queue field.
pub struct MyApp<DBH: db::Handler, M: mail::Handler, S: storage::Handler> {
    pub db: DBH,
    pub mail: M,
    pub storage: S,
    pub response_lag: u64,
}

pub fn new<DBH: db::Handler, M: mail::Handler, S: storage::Handler>(
    db: DBH,
    mail: M,
    storage: S,
    response_lag: u64,
) -> MyApp<DBH, M, S> {
    MyApp {
        db,
        mail,
        storage,
        response_lag,
    }
}

impl<DBH: db::Handler, M: mail::Handler, S: storage::Handler> App for MyApp<DBH, M, S> {
    fn response_lag(&self) -> u64 {
        self.response_lag
    }
}

impl<DBH: db::Handler, M: mail::Handler, S: storage::Handler> HasDb for MyApp<DBH, M, S> {
    fn db(&self) -> &impl db::Handler {
        &self.db
    }
}

impl<DBH: db::Handler, M: mail::Handler, S: storage::Handler> HasMail for MyApp<DBH, M, S> {
    fn mail(&self) -> &impl mail::Handler {
        &self.mail
    }
}

impl<DBH: db::Handler, M: mail::Handler, S: storage::Handler> HasStorage for MyApp<DBH, M, S> {
    fn storage(&self) -> &impl storage::Handler {
        &self.storage
    }
}
