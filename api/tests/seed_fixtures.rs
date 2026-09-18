//! Invariants of `local/seed/synthetic.json` — the committed fixture applied
//! by `make local-seed`. JSON-only, no DB and no network: this test reads the
//! file and checks it against itself.
//!
//! This is the backstop that keeps real addresses (and other unintended
//! coupling) out of a public repo: a token's plaintext (documented in
//! `DEVELOPMENT.md`) must match its stored hash, every id a row references
//! must resolve to a row that exists, and — the hard constraint — every
//! email-shaped string anywhere in the fixture must be `@example.com` or
//! `@microticket.test` (a subdomain of either is fine; a `*@domain` wildcard
//! counts as its domain).

use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The plaintext tokens DEVELOPMENT.md tells people to use. The fixture
/// stores only their sha256 hashes, exactly as production does, so this is
/// the only place the two can be compared.
const USER_TOKENS: &[(&str, &str)] = &[
    ("SeedOwnerUsr", "mtu_localdev0000000000000000000owner"),
    ("SeedAgentUsr", "mtu_localdev0000000000000000000agent"),
];

/// Human-facing identifiers DEVELOPMENT.md hands out as things to type or
/// navigate to — slugs and emails, not opaque record ids (DEVELOPMENT.md
/// identifies the seeded instances/users by slug/email, the same way a person
/// actually running the stack would). Rename one and the docs must be
/// renamed with it.
const DOCUMENTED_IDS: &[&str] = &[
    "acme",
    "ridgeline",
    "owner@microticket.test",
    "agent@microticket.test",
];

/// Domain roots every fixture email/address must be under (exactly, or as a
/// subdomain of). This repo is public — see the module doc.
const ALLOWED_DOMAIN_ROOTS: &[&str] = &["example.com", "microticket.test"];

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

fn read_json(rel: &str) -> Value {
    let path = repo_root().join(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn synthetic() -> Value {
    read_json("local/seed/synthetic.json")
}

fn rows<'a>(doc: &'a Value, table: &str) -> &'a [Value] {
    doc["tables"][table]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// The `{"S": "..."}` wire shape the fixture is written in.
fn s(row: &Value, attr: &str) -> Option<String> {
    row.get(attr)?.get("S")?.as_str().map(str::to_owned)
}

fn id(row: &Value) -> String {
    s(row, "id").expect("every fixture row with an `id` needs one")
}

#[test]
fn synthetic_references_resolve() {
    let doc = synthetic();
    let instances: HashSet<String> = rows(&doc, "instance").iter().map(id).collect();
    let users: HashSet<String> = rows(&doc, "user").iter().map(id).collect();

    for addr in rows(&doc, "inbound_address") {
        let instance_id = s(addr, "instance_id").expect("inbound_address.instance_id");
        assert!(
            instances.contains(&instance_id),
            "inbound_address {} points at missing instance {instance_id}",
            s(addr, "address").unwrap_or_default()
        );
    }

    for membership in rows(&doc, "membership") {
        let instance_id = s(membership, "instance_id").expect("membership.instance_id");
        let user_id = s(membership, "user_id").expect("membership.user_id");
        assert!(
            instances.contains(&instance_id),
            "membership {} points at missing instance {instance_id}",
            id(membership)
        );
        assert!(
            users.contains(&user_id),
            "membership {} points at missing user {user_id}",
            id(membership)
        );
    }

    for token in rows(&doc, "user_token") {
        let user_id = s(token, "user_id").expect("user_token.user_id");
        assert!(
            users.contains(&user_id),
            "user_token {} points at missing user {user_id}",
            id(token)
        );
    }

    for ticket in rows(&doc, "ticket") {
        let instance_id = s(ticket, "instance_id").expect("ticket.instance_id");
        assert!(
            instances.contains(&instance_id),
            "ticket {} points at missing instance {instance_id}",
            id(ticket)
        );
        if let Some(assignee) = s(ticket, "assignee_user_id") {
            assert!(
                users.contains(&assignee),
                "ticket {} points at missing assignee {assignee}",
                id(ticket)
            );
        }
    }

    let tickets: HashSet<String> = rows(&doc, "ticket").iter().map(id).collect();
    for message in rows(&doc, "ticket_message") {
        let ticket_id = s(message, "ticket_id").expect("ticket_message.ticket_id");
        assert!(
            tickets.contains(&ticket_id),
            "ticket_message {} points at missing ticket {ticket_id}",
            id(message)
        );
    }

    for counter in rows(&doc, "counter") {
        assert!(
            instances.contains(&id(counter)),
            "counter {} (row id doubles as instance id) points at a missing instance",
            id(counter)
        );
    }
}

/// The specific ticket states the build plan asks the fixture to cover:
/// open, closed, deleted, assigned, unassigned, multiple requesters+CC, and
/// (via `ticket_message`) an internal note — see `SCHEMA.md`/the step-5 task.
#[test]
fn synthetic_tickets_cover_every_required_state() {
    let doc = synthetic();
    let tickets = rows(&doc, "ticket");

    let statuses: HashSet<String> = tickets.iter().filter_map(|t| s(t, "status")).collect();
    for want in ["open", "closed", "deleted"] {
        assert!(
            statuses.contains(want),
            "expected at least one seeded ticket with status {want:?}"
        );
    }

    assert!(
        tickets.iter().any(|t| s(t, "assignee_user_id").is_some()),
        "expected at least one assigned ticket"
    );
    assert!(
        tickets.iter().any(|t| s(t, "assignee_user_id").is_none()),
        "expected at least one unassigned ticket"
    );
    assert!(
        tickets.iter().any(|t| {
            t.get("requester_emails")
                .and_then(|v| v["SS"].as_array())
                .is_some_and(|a| a.len() > 1)
        }),
        "expected at least one ticket with multiple requesters"
    );
    assert!(
        tickets.iter().any(|t| t.get("cc_emails").is_some()),
        "expected at least one ticket with a CC"
    );

    let note_exists = rows(&doc, "ticket_message")
        .iter()
        .any(|m| s(m, "kind").as_deref() == Some("note"));
    assert!(
        note_exists,
        "expected at least one seeded ticket_message with kind: note"
    );
}

#[test]
fn instance_names_do_not_substring_match_each_other() {
    // So a name-based selector in a script or test can't accidentally hit
    // both — see the doc comment on the plan this fixture implements.
    let doc = synthetic();
    let instances = rows(&doc, "instance");
    for (i, a) in instances.iter().enumerate() {
        for b in &instances[i + 1..] {
            let name_a = s(a, "name").expect("instance.name").to_lowercase();
            let name_b = s(b, "name").expect("instance.name").to_lowercase();
            assert!(
                !name_a.contains(&name_b) && !name_b.contains(&name_a),
                "instance names {name_a:?} and {name_b:?} substring-match each other"
            );
        }
    }
}

#[test]
fn exactly_one_instance_has_public_submission_enabled() {
    let doc = synthetic();
    let public_count = rows(&doc, "instance")
        .iter()
        .filter(|i| {
            i.get("public_submission_enabled")
                .and_then(|v| v["BOOL"].as_bool())
                == Some(true)
        })
        .count();
    assert_eq!(
        public_count, 1,
        "expected exactly one seeded instance with public_submission_enabled"
    );
}

#[test]
fn at_least_one_inbound_address_is_a_wildcard() {
    let doc = synthetic();
    assert!(
        rows(&doc, "inbound_address")
            .iter()
            .any(|a| s(a, "kind").as_deref() == Some("wildcard")),
        "expected at least one wildcard (kind: wildcard) inbound_address row"
    );
}

#[test]
fn there_is_an_owner_and_an_agent() {
    let doc = synthetic();
    let roles: HashSet<String> = rows(&doc, "membership")
        .iter()
        .filter_map(|m| s(m, "role"))
        .collect();
    assert!(
        roles.contains("owner"),
        "expected a seeded owner membership"
    );
    assert!(
        roles.contains("agent"),
        "expected a seeded agent membership"
    );
}

#[test]
fn user_token_hashes_match_their_documented_plaintexts() {
    let doc = synthetic();
    for (user_id, plaintext) in USER_TOKENS {
        let token = rows(&doc, "user_token")
            .iter()
            .find(|t| s(t, "user_id").as_deref() == Some(*user_id))
            .unwrap_or_else(|| panic!("no user_token seeded for {user_id}"));

        let want = hex::encode(Sha256::digest(plaintext.as_bytes()));
        assert_eq!(
            s(token, "token_hash").expect("token_hash"),
            want,
            "user_token for {user_id} does not hash the plaintext DEVELOPMENT.md documents"
        );
        assert!(
            plaintext.starts_with(microticket::auth::USER_TOKEN_PREFIX),
            "documented plaintext for {user_id} does not carry the mtu_ prefix"
        );
    }
}

#[test]
fn documented_ids_still_exist() {
    let synthetic_text = std::fs::read_to_string(repo_root().join("local/seed/synthetic.json"))
        .expect("reading synthetic.json");
    let docs = ["DEVELOPMENT.md", "CLAUDE.md"].map(|f| {
        std::fs::read_to_string(repo_root().join(f)).unwrap_or_else(|e| panic!("reading {f}: {e}"))
    });

    for name in DOCUMENTED_IDS {
        assert!(
            synthetic_text.contains(name),
            "{name} is documented but no longer in synthetic.json"
        );
        assert!(
            docs.iter().any(|d| d.contains(name)),
            "{name} is in the fixture but named in neither DEVELOPMENT.md nor CLAUDE.md"
        );
    }

    for (_, plaintext) in USER_TOKENS {
        assert!(
            docs[0].contains(plaintext),
            "DEVELOPMENT.md no longer documents the token {plaintext}"
        );
    }
}

/// The domain of an email-shaped `local@domain` string, or `None` if it
/// doesn't look like one at all.
fn email_domain(candidate: &str) -> Option<&str> {
    let (local, domain) = candidate.split_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return None;
    }
    // A bare wildcard local part ("*") is still address-shaped for this
    // purpose — inbound_address rows use it.
    if !local
        .chars()
        .all(|c| c == '*' || c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
    {
        return None;
    }
    if !domain
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
    {
        return None;
    }
    Some(domain)
}

fn domain_is_allowed(domain: &str) -> bool {
    ALLOWED_DOMAIN_ROOTS
        .iter()
        .any(|root| domain == *root || domain.ends_with(&format!(".{root}")))
}

/// Walk every string value in a JSON document, calling `f` with each one.
fn walk_strings(value: &Value, f: &mut impl FnMut(&str)) {
    match value {
        Value::String(s) => f(s),
        Value::Array(items) => items.iter().for_each(|v| walk_strings(v, f)),
        Value::Object(map) => map.values().for_each(|v| walk_strings(v, f)),
        _ => {}
    }
}

#[test]
fn every_address_is_example_com_or_microticket_test() {
    let doc = synthetic();
    let mut checked = 0usize;
    let mut violations = Vec::new();
    walk_strings(&doc, &mut |s| {
        // Split on whitespace too: `_comment` fields are prose, not a single
        // address, so scan each whitespace-separated token independently
        // rather than requiring the whole string to be address-shaped.
        for token in s.split_whitespace() {
            let token =
                token.trim_matches(|c: char| matches!(c, '(' | ')' | ',' | '.' | ';' | ':'));
            if let Some(domain) = email_domain(token) {
                checked += 1;
                if !domain_is_allowed(domain) {
                    violations.push(token.to_string());
                }
            }
        }
    });
    assert!(
        checked > 0,
        "found no email-shaped strings in synthetic.json — has the fixture format changed?"
    );
    assert!(
        violations.is_empty(),
        "found address(es) outside @example.com/@microticket.test: {violations:?}"
    );
}
