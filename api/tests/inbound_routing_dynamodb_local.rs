//! Integration test for `inbound::routing::resolve_instance_id` against **DynamoDB
//! Local** — proving the address resolver finds the right instance for an exact
//! address, a `+tag` address, and a wildcard address, end to end through a real
//! `inbound_address` table (not the pure-function unit tests in
//! `src/inbound/routing.rs`, which stand in a fake lookup instead of a database).
//!
//! # Running this test
//!
//! ```sh
//! make local-up
//! make local-tables
//! cd api
//! set -a && . ../local/local.env && set +a
//! cargo test --test inbound_routing_dynamodb_local
//! ```
//!
//! Like `tests/auth_dynamodb_local.rs`, every test here **skips itself**
//! (prints a message, returns) when no reachable local DynamoDB is configured,
//! so `cargo test` and CI (which never brings up DynamoDB Local for the
//! `api-check` job) stay green with no local stack running.

use microticket::db::{self, Handler as _};
use microticket::dynamodb;
use microticket::inbound::routing;

/// `Some(prefix)` when a local DynamoDB is configured *and* reachable; `None`
/// means every test below should skip itself. See
/// `tests/auth_dynamodb_local.rs`'s identically-named helper for the full
/// rationale.
async fn local_db_prefix() -> Option<String> {
    let endpoint = microticket::local_dev::require_local_dynamodb_endpoint().ok()?;
    let prefix = std::env::var("DB_PREFIX").ok()?;

    let client = microticket::local_dev::dynamodb_client().await;
    if client.list_tables().send().await.is_err() {
        eprintln!(
            "inbound_routing_dynamodb_local: {endpoint} is configured but not reachable — \
             skipping. Run `make local-up && make local-tables` first."
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
                    "inbound_routing_dynamodb_local: AWS_ENDPOINT_URL_DYNAMODB/DB_PREFIX not set \
                     to a reachable local DynamoDB — skipping. See this file's header for how to \
                     run it."
                );
                return;
            }
        }
    };
}

/// A fresh domain, guaranteed lowercase — `create_inbound_address` stores
/// whatever it's given verbatim (callers are expected to normalize first, per
/// its doc comment), and `nanoid::nanoid!`'s default alphabet is mixed-case,
/// so this must lowercase explicitly rather than assume the generated id
/// already is.
fn unique_domain(label: &str) -> String {
    format!("{label}-{}.microticket.test", nanoid::nanoid!(10)).to_lowercase()
}

#[tokio::test]
async fn resolves_an_exact_address() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, /* read_only */ false).await;

    let instance = db
        .create_instance(
            "Exact Co",
            &format!("exact-{}", nanoid::nanoid!(8)),
            "Exact Co",
            "",
            false,
        )
        .await
        .expect("create_instance");
    let domain = unique_domain("exact");
    let address = format!("support@{domain}");
    db.create_inbound_address(&address, &instance.id, microticket::db::AddressKind::Exact)
        .await
        .expect("create_inbound_address");

    let resolved = routing::resolve_instance_id(&db, std::slice::from_ref(&address))
        .await
        .expect("resolve_instance_id");
    assert_eq!(resolved.as_deref(), Some(instance.id.as_str()));
}

#[tokio::test]
async fn resolves_a_plus_tag_address_by_stripping_the_tag() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;

    let instance = db
        .create_instance(
            "Tag Co",
            &format!("tag-{}", nanoid::nanoid!(8)),
            "Tag Co",
            "",
            false,
        )
        .await
        .expect("create_instance");
    let domain = unique_domain("tag");
    let base_address = format!("support@{domain}");
    db.create_inbound_address(
        &base_address,
        &instance.id,
        microticket::db::AddressKind::Exact,
    )
    .await
    .expect("create_inbound_address");

    let tagged = format!("support+t1a2b3c@{domain}");
    let resolved = routing::resolve_instance_id(&db, &[tagged])
        .await
        .expect("resolve_instance_id");
    assert_eq!(resolved.as_deref(), Some(instance.id.as_str()));
}

#[tokio::test]
async fn resolves_via_a_wildcard() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;

    let instance = db
        .create_instance(
            "Wildcard Co",
            &format!("wild-{}", nanoid::nanoid!(8)),
            "Wildcard Co",
            "",
            false,
        )
        .await
        .expect("create_instance");
    let domain = unique_domain("wild");
    let wildcard = format!("*@{domain}");
    db.create_inbound_address(
        &wildcard,
        &instance.id,
        microticket::db::AddressKind::Wildcard,
    )
    .await
    .expect("create_inbound_address");

    let recipient = format!("anything-at-all@{domain}");
    let resolved = routing::resolve_instance_id(&db, &[recipient])
        .await
        .expect("resolve_instance_id");
    assert_eq!(resolved.as_deref(), Some(instance.id.as_str()));
}

#[tokio::test]
async fn an_exact_address_wins_over_a_wildcard_on_the_same_domain() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;

    let exact_instance = db
        .create_instance(
            "Exact Wins",
            &format!("exact-wins-{}", nanoid::nanoid!(8)),
            "Exact Wins",
            "",
            false,
        )
        .await
        .expect("create_instance");
    let wildcard_instance = db
        .create_instance(
            "Wildcard Loses",
            &format!("wild-loses-{}", nanoid::nanoid!(8)),
            "Wildcard Loses",
            "",
            false,
        )
        .await
        .expect("create_instance");

    let domain = unique_domain("both");
    let exact_address = format!("support@{domain}");
    db.create_inbound_address(
        &exact_address,
        &exact_instance.id,
        microticket::db::AddressKind::Exact,
    )
    .await
    .expect("create exact");
    db.create_inbound_address(
        &format!("*@{domain}"),
        &wildcard_instance.id,
        microticket::db::AddressKind::Wildcard,
    )
    .await
    .expect("create wildcard");

    let resolved = routing::resolve_instance_id(&db, &[exact_address])
        .await
        .expect("resolve_instance_id");
    assert_eq!(resolved.as_deref(), Some(exact_instance.id.as_str()));
}

#[tokio::test]
async fn no_match_across_multiple_recipients_is_none() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;

    let domain = unique_domain("nomatch");
    let resolved = routing::resolve_instance_id(
        &db,
        &[format!("nobody@{domain}"), format!("also-nobody@{domain}")],
    )
    .await
    .expect("resolve_instance_id");
    assert_eq!(resolved, None);
}

#[tokio::test]
async fn only_one_of_several_recipients_being_ours_still_resolves() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;

    let instance = db
        .create_instance(
            "Multi Recipient Co",
            &format!("multi-{}", nanoid::nanoid!(8)),
            "Multi Recipient Co",
            "",
            false,
        )
        .await
        .expect("create_instance");
    let domain = unique_domain("multi");
    let ours = format!("support@{domain}");
    db.create_inbound_address(&ours, &instance.id, microticket::db::AddressKind::Exact)
        .await
        .expect("create_inbound_address");

    let unrelated_domain = unique_domain("unrelated");
    let resolved = routing::resolve_instance_id(
        &db,
        &[format!("someone-else@{unrelated_domain}"), ours.clone()],
    )
    .await
    .expect("resolve_instance_id");
    assert_eq!(resolved.as_deref(), Some(instance.id.as_str()));
}

/// `--instance` on the CLI takes a slug, but every row that references an
/// instance stores its **id**. Passing the argument through unresolved wrote the
/// slug into `instance_id`, which fails nowhere: the row is created, the CLI
/// prints it back, and nothing notices until a resolver loads the instance by
/// that id and finds nothing. In production the visible symptom was an empty
/// instance switcher for a user whose membership plainly existed.
#[tokio::test]
async fn resolve_instance_id_maps_a_slug_to_the_id_rows_must_store() {
    let prefix = require_local_db!();
    let db = dynamodb::Handler::new(&prefix, false).await;

    let slug = format!("resolve-{}", nanoid::nanoid!(8)).to_lowercase();
    let instance = db
        .create_instance("Resolve Test", &slug, "Resolve Test", "", false)
        .await
        .expect("create instance");

    // The slug and the id are different strings — the whole point.
    assert_ne!(instance.id, slug);

    assert_eq!(
        db::resolve_instance_id(&db, &slug).await.expect("by slug"),
        instance.id,
        "a slug must resolve to the instance id, never pass through as itself"
    );
    assert_eq!(
        db::resolve_instance_id(&db, &instance.id)
            .await
            .expect("by id"),
        instance.id,
        "an id must resolve to itself"
    );
    assert!(
        db::resolve_instance_id(&db, "definitely-not-an-instance")
            .await
            .is_err(),
        "an unknown slug or id must be rejected, not persisted"
    );
}
