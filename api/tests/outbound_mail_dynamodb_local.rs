//! Integration tests for outbound mail (step 6) against **DynamoDB Local**
//! and the **mock** mailer — not real SES. Proves the full shape end to end,
//! through the real GraphQL mutations, in a way none of the pure unit tests
//! in `src/outbound.rs` can: that `replyToTicket` actually calls
//! `mail::Handler::send_raw` exactly once, addressed correctly, carrying the
//! `+t{token}` reply address and the tagged subject; that `addInternalNote`
//! calls it zero times; that `setTicketStatus` sends a notice on close; and
//! that `submitTicket` sends an acknowledgement.
//!
//! # Running this test
//!
//! ```sh
//! make local-up
//! make local-tables
//! cd api
//! set -a && . ../local/local.env && set +a
//! cargo test --test outbound_mail_dynamodb_local
//! ```
//!
//! Like the other `*_dynamodb_local.rs` files, every test here **skips
//! itself** when no reachable local DynamoDB is configured, so `cargo test`
//! and CI stay green with no local stack running.

use std::sync::Arc;

use async_graphql::{Request, Response, Variables};
use microticket::app;
use microticket::auth::{AuthInfo, Membership};
use microticket::db;
use microticket::db::Handler as _;
use microticket::dynamodb;
use microticket::graphql;
use microticket::mockmail;
use serde_json::json;

type TestApp = app::MyApp<dynamodb::Handler, mockmail::Handler>;
type TestSchema = graphql::MicroticketSchema<TestApp>;

/// See `tests/auth_dynamodb_local.rs`'s identically-named helper for the
/// full rationale.
async fn local_db_prefix() -> Option<String> {
    let endpoint = microticket::local_dev::require_local_dynamodb_endpoint().ok()?;
    let prefix = std::env::var("DB_PREFIX").ok()?;

    let client = microticket::local_dev::dynamodb_client().await;
    if client.list_tables().send().await.is_err() {
        eprintln!(
            "outbound_mail_dynamodb_local: {endpoint} is configured but not reachable — skipping. \
             Run `make local-up && make local-tables` first."
        );
        return None;
    }

    Some(prefix)
}

macro_rules! require_local_db {
    () => {
        match local_db_prefix().await {
            Some(prefix) => prefix,
            None => {
                eprintln!(
                    "outbound_mail_dynamodb_local: AWS_ENDPOINT_URL_DYNAMODB/DB_PREFIX not set to \
                     a reachable local DynamoDB — skipping. See this file's header for how to run it."
                );
                return;
            }
        }
    };
}

fn unique_id(label: &str) -> String {
    format!("{label}-{}", nanoid::nanoid!(8))
}

fn unique_email(label: &str) -> String {
    format!("{label}-{}@example.com", nanoid::nanoid!(8))
}

fn expect_data(response: &Response, path: &str) -> serde_json::Value {
    assert!(
        response.errors.is_empty(),
        "unexpected GraphQL errors on {path}: {:?}",
        response.errors
    );
    let data = serde_json::to_value(&response.data).expect("response data serializes");
    let mut cur = &data;
    for segment in path.split('.') {
        cur = cur
            .get(segment)
            .unwrap_or_else(|| panic!("response data missing `{segment}` (path {path}): {data}"));
    }
    cur.clone()
}

fn user_auth(user_id: &str, instance_id: &str, is_owner: bool) -> AuthInfo {
    AuthInfo::User {
        id: user_id.to_string(),
        memberships: vec![Membership {
            instance_id: instance_id.to_string(),
            is_owner,
        }],
        token_id: None,
    }
}

fn requester_auth(email: &str, instance_id: &str) -> AuthInfo {
    AuthInfo::Requester {
        email: email.to_string(),
        instance_id: instance_id.to_string(),
    }
}

/// An instance with one exact inbound address, one owner user, and a
/// membership — every test's starting point. Returns
/// `(instance, owner_user_id, inbound_address)`.
async fn setup_instance(db: &dynamodb::Handler, label: &str) -> (db::Instance, String, String) {
    let instance = db
        .create_instance(
            &format!("{label} instance"),
            &unique_id(&format!("{label}-slug")),
            &format!("{label} Support"),
            "Thanks,\nThe Support Team",
            false,
        )
        .await
        .expect("create_instance");
    let address = format!("support-{}@{label}.microticket.test", nanoid::nanoid!(6));
    db.create_inbound_address(&address, &instance.id, db::AddressKind::Exact)
        .await
        .expect("create_inbound_address");
    let owner = db
        .create_user(&unique_email(&format!("{label}-owner")), "Owner")
        .await
        .expect("create_user");
    db.create_membership(&owner.id, &instance.id, db::MembershipRole::Owner)
        .await
        .expect("create_membership");
    (instance, owner.id, address)
}

/// Create a ticket directly through the DB layer, with a requester and one
/// CC, returning the created `db::Ticket`.
async fn make_ticket(
    db: &dynamodb::Handler,
    instance_id: &str,
    requester_email: &str,
    cc_email: &str,
) -> db::Ticket {
    let number = db
        .increment_ticket_counter(instance_id)
        .await
        .expect("increment_ticket_counter");
    db.create_ticket(
        instance_id,
        number,
        "Where is my order?",
        std::slice::from_ref(&requester_email.to_string()),
        std::slice::from_ref(&cc_email.to_string()),
    )
    .await
    .expect("create_ticket")
}

fn build_app_and_schema(db: dynamodb::Handler) -> (Arc<TestApp>, TestSchema) {
    let my_app = Arc::new(app::new(db, mockmail::Handler::new(), 0));
    let webauthn = Arc::new(app::build_webauthn().expect("WebAuthn build failed"));
    let schema = graphql::build_schema(my_app.clone(), webauthn);
    (my_app, schema)
}

const REPLY_MUTATION: &str = r#"
    mutation($ticketId: ID!, $body: String!) {
        replyToTicket(ticketId: $ticketId, body: $body) { id kind rfcMessageId }
    }
"#;

const ADD_NOTE_MUTATION: &str = r#"
    mutation($ticketId: ID!, $body: String!) {
        addInternalNote(ticketId: $ticketId, body: $body) { id kind }
    }
"#;

const SET_STATUS_MUTATION: &str = r#"
    mutation($ticketId: ID!, $status: TicketStatusType!) {
        setTicketStatus(ticketId: $ticketId, status: $status) { id status }
    }
"#;

const SUBMIT_TICKET_MUTATION: &str = r#"
    mutation($subject: String!, $body: String!) {
        submitTicket(subject: $subject, body: $body) { id number }
    }
"#;

#[tokio::test]
async fn reply_sends_exactly_one_message_addressed_and_tagged_correctly() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;
    let (instance, owner_id, _address) = setup_instance(&db, "reply").await;
    let requester_email = unique_email("requester");
    let cc_email = unique_email("cc");
    let ticket = make_ticket(&db, &instance.id, &requester_email, &cc_email).await;

    let (my_app, schema) = build_app_and_schema(db);

    let response = schema
        .execute(
            Request::new(REPLY_MUTATION)
                .variables(Variables::from_json(json!({
                    "ticketId": ticket.id,
                    "body": "We're looking into it.",
                })))
                .data(user_auth(&owner_id, &instance.id, true)),
        )
        .await;
    let rfc_message_id = expect_data(&response, "replyToTicket.rfcMessageId");
    assert!(
        rfc_message_id.as_str().is_some_and(|s| !s.is_empty()),
        "the reply must be stamped with an rfcMessageId once sent: {rfc_message_id:?}"
    );

    let sent = my_app.mail.sent_raw();
    assert_eq!(sent.len(), 1, "exactly one message must be sent: {sent:?}");
    let msg = &sent[0];
    assert_eq!(msg.to, vec![requester_email.clone()]);
    assert_eq!(msg.cc, vec![cc_email.clone()]);

    let raw = String::from_utf8_lossy(&msg.raw);
    let expected_tag = format!("[#{}-{}]", instance.slug, ticket.number);
    assert!(
        raw.contains(&expected_tag),
        "subject must carry {expected_tag:?}: {raw}"
    );
    assert!(
        raw.contains(&format!("+t{}@", ticket.reply_token)),
        "Reply-To must carry the +t{{token}} address: {raw}"
    );
    assert!(raw.contains("X-Microticket-Loop: 1"), "{raw}");
}

#[tokio::test]
async fn internal_note_sends_no_mail() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;
    let (instance, owner_id, _address) = setup_instance(&db, "note").await;
    let requester_email = unique_email("requester");
    let cc_email = unique_email("cc");
    let ticket = make_ticket(&db, &instance.id, &requester_email, &cc_email).await;

    let (my_app, schema) = build_app_and_schema(db);

    let response = schema
        .execute(
            Request::new(ADD_NOTE_MUTATION)
                .variables(Variables::from_json(json!({
                    "ticketId": ticket.id,
                    "body": "Internal only: waiting on vendor.",
                })))
                .data(user_auth(&owner_id, &instance.id, true)),
        )
        .await;
    assert_eq!(
        expect_data(&response, "addInternalNote.kind"),
        json!("NOTE")
    );

    assert_eq!(
        my_app.mail.sent_raw().len(),
        0,
        "an internal note must never send mail"
    );
    assert_eq!(
        my_app.mail.sent().len(),
        0,
        "an internal note must never send mail (plain/HTML path either)"
    );
}

#[tokio::test]
async fn closing_a_ticket_sends_exactly_one_notice() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;
    let (instance, owner_id, _address) = setup_instance(&db, "close").await;
    let requester_email = unique_email("requester");
    let cc_email = unique_email("cc");
    let ticket = make_ticket(&db, &instance.id, &requester_email, &cc_email).await;

    let (my_app, schema) = build_app_and_schema(db);

    let response = schema
        .execute(
            Request::new(SET_STATUS_MUTATION)
                .variables(Variables::from_json(json!({
                    "ticketId": ticket.id,
                    "status": "CLOSED",
                })))
                .data(user_auth(&owner_id, &instance.id, true)),
        )
        .await;
    assert_eq!(
        expect_data(&response, "setTicketStatus.status"),
        json!("CLOSED")
    );

    let sent = my_app.mail.sent_raw();
    assert_eq!(
        sent.len(),
        1,
        "closing must send exactly one notice: {sent:?}"
    );
    assert_eq!(sent[0].to, vec![requester_email]);
    assert_eq!(sent[0].cc, vec![cc_email]);
}

#[tokio::test]
async fn assigning_a_ticket_sends_no_mail() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;
    let (instance, owner_id, _address) = setup_instance(&db, "assign").await;
    let requester_email = unique_email("requester");
    let cc_email = unique_email("cc");
    let ticket = make_ticket(&db, &instance.id, &requester_email, &cc_email).await;

    let (my_app, schema) = build_app_and_schema(db);

    const ASSIGN_MUTATION: &str = r#"
        mutation($ticketId: ID!, $userId: ID) {
            assignTicket(ticketId: $ticketId, userId: $userId) { id assigneeUserId }
        }
    "#;
    let response = schema
        .execute(
            Request::new(ASSIGN_MUTATION)
                .variables(Variables::from_json(json!({
                    "ticketId": ticket.id,
                    "userId": owner_id,
                })))
                .data(user_auth(&owner_id, &instance.id, true)),
        )
        .await;
    assert_eq!(
        expect_data(&response, "assignTicket.assigneeUserId"),
        json!(owner_id)
    );

    assert_eq!(
        my_app.mail.sent_raw().len(),
        0,
        "assignment is internal bookkeeping and must never send mail"
    );
}

#[tokio::test]
async fn submitting_a_ticket_sends_an_acknowledgement() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;
    let (instance, _owner_id, _address) = setup_instance(&db, "submit").await;
    let requester_email = unique_email("requester");

    let (my_app, schema) = build_app_and_schema(db);

    let response = schema
        .execute(
            Request::new(SUBMIT_TICKET_MUTATION)
                .variables(Variables::from_json(json!({
                    "subject": "Help please",
                    "body": "Something is broken.",
                })))
                .data(requester_auth(&requester_email, &instance.id)),
        )
        .await;
    assert!(
        expect_data(&response, "submitTicket.id").as_str().is_some(),
        "submitTicket must succeed even though mail is best-effort"
    );

    let sent = my_app.mail.sent_raw();
    assert_eq!(
        sent.len(),
        1,
        "submitTicket must send exactly one acknowledgement: {sent:?}"
    );
    assert_eq!(sent[0].to, vec![requester_email]);
}
