//! The inbound-mail Lambda (`inbound-mail-lambda`), triggered by the SQS
//! queue an SNS topic feeds from SES's receipt rule.
//!
//! Wiring, per the build plan:
//! ```text
//! MX → SES receipt rule set
//!       ├─ S3 action  → s3://{mail bucket}/inbound/{ses_message_id}
//!       └─ SNS action → SNS topic → SQS queue (+ DLQ, 3 retries) → this Lambda
//! ```
//!
//! Each SQS record's body is an SNS notification envelope; its `Message`
//! field is itself a JSON-encoded SES receipt event
//! (`aws_lambda_events::event::ses::SimpleEmailService`) carrying
//! `mail.message_id` — this Lambda never trusts anything else in that event
//! for the message *content*, only for the id: the raw MIME bytes always
//! come from `s3://{bucket}/inbound/{message_id}`, the exact key the S3
//! receipt action wrote them to (a fixed prefix with no per-instance
//! branching, since routing happens after parsing, not before).
//!
//! All the actual work — idempotency, parsing, loop guards, resolution,
//! storage, notification — lives in `microticket::inbound::pipeline`,
//! shared with `bin/cli.rs`'s `mail process` (`make local-mail`).

use anyhow::{Context, Result, anyhow};
use aws_lambda_events::event::ses::SimpleEmailService;
use aws_lambda_events::event::sqs::SqsEvent;
use lambda_runtime::{Error, LambdaEvent, run, service_fn, tracing};
use serde::Deserialize;
use std::sync::Arc;

use microticket::app;
use microticket::app::HasStorage as _;
use microticket::dynamodb;
use microticket::inbound::pipeline;
use microticket::s3storage;
use microticket::sesmail;

/// The outer envelope every SQS message body carries: an SNS notification.
/// Only `Message` (the inner, JSON-encoded SES event) is needed here.
#[derive(Deserialize)]
struct SnsEnvelope {
    #[serde(rename = "Message")]
    message: String,
}

/// The fixed S3 key prefix the receipt rule's S3 action writes raw MIME
/// under — see this module's doc comment. Not a Terraform output or an env
/// var: it's a convention fixed by `infra/ses.tf`'s S3 action config, and
/// baked in on both sides.
const RAW_MAIL_PREFIX: &str = "inbound";

type App = app::MyApp<dynamodb::Handler, sesmail::Mailer, s3storage::Storage>;

async fn handle_record(app: &App, body: &str) -> Result<()> {
    let envelope: SnsEnvelope =
        serde_json::from_str(body).context("parsing SQS record body as an SNS envelope")?;
    let ses_event: SimpleEmailService = serde_json::from_str(&envelope.message)
        .context("parsing SNS Message as an SES receipt event")?;
    let message_id = ses_event
        .mail
        .message_id
        .ok_or_else(|| anyhow!("SES event has no mail.message_id"))?;

    let raw_key = format!("{RAW_MAIL_PREFIX}/{message_id}");
    let raw = {
        use microticket::storage::Handler as _;
        app.storage()
            .get_bytes(&raw_key)
            .await
            .with_context(|| format!("fetching raw MIME from s3 key {raw_key}"))?
    };

    let outcome = pipeline::process_raw_message(app, &message_id, &raw, Some(&raw_key)).await?;
    if let Some(reason) = &outcome.dropped_reason {
        tracing::info!(message_id, reason, "inbound message dropped");
    } else {
        tracing::info!(
            message_id,
            ticket_id = outcome.ticket_id.as_deref().unwrap_or_default(),
            new_ticket = outcome.created_new_ticket,
            attachments = outcome.attachment_count,
            "inbound message processed"
        );
    }
    Ok(())
}

async fn function_handler(app: Arc<App>, event: LambdaEvent<SqsEvent>) -> Result<(), Error> {
    let mut last_err: Option<anyhow::Error> = None;
    for record in event.payload.records {
        let Some(body) = record.body else {
            tracing::warn!("SQS record has no body; skipping");
            continue;
        };
        if let Err(e) = handle_record(&app, &body).await {
            tracing::error!(error = %e, "failed to process an inbound-mail SQS record");
            last_err = Some(e);
        }
    }
    // `batch_size = 1` (per the build plan's `sqs.tf` sketch) means there is
    // normally exactly one record here, so surfacing the last error fails
    // the whole invocation and lets SQS redeliver — up to the queue's
    // `maxReceiveCount` before landing in the DLQ. If `batch_size` is ever
    // raised, this would redeliver every record in the batch on one
    // failure; revisit with a partial-batch-failure response
    // (`ReportBatchItemFailures`) at that point.
    match last_err {
        Some(e) => Err(e.into()),
        None => Ok(()),
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    let mailer = sesmail::Mailer::new().await;
    let storage = s3storage::Storage::new()
        .await
        .expect("MAIL_BUCKET must be set for the inbound-mail lambda");

    let read_only = std::env::var("READ_ONLY")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let db_prefix =
        std::env::var("DB_PREFIX").expect("DB_PREFIX must be set for the dynamodb backend");
    let db = dynamodb::Handler::new(&db_prefix, read_only).await;

    let app = Arc::new(app::new(db, mailer, storage, 0));

    run(service_fn(move |event| {
        let app = app.clone();
        async move { function_handler(app, event).await }
    }))
    .await
}
