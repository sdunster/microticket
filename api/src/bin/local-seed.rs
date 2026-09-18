//! Populate a local DynamoDB with enough data to actually use the app:
//! `apply` writes `local/seed/synthetic.json`'s rows into the local
//! database; `clear` deletes the rows the running app itself writes (session
//! tokens, WebAuthn state, ...) that no fixture owns, leaving the seeded rows
//! alone.
//!
//! Unlike seslogin's `local-seed`, there is no `extract` step: seslogin pulls
//! *reference* data (categories, NITC groups) out of a real database because
//! that data isn't something a fixture should invent. microticket has no such
//! table — everything `synthetic.json` describes (instances, addresses,
//! users, memberships, tokens) is invented outright, so there is nothing to
//! extract and nothing that ever needs real AWS credentials here.
//!
//! Rows are written as **raw DynamoDB items**, not through `db::Handler` —
//! that preserves the ids `synthetic.json` hard-codes (which
//! `db::Handler::create_*` would otherwise regenerate) and the exact
//! attribute shape, including the omit-optional-attributes-never-`Null` house
//! rule.
//!
//! Refuses to run against anything but a local DynamoDB, via
//! `local_dev::require_local_dynamodb_endpoint()`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use aws_sdk_dynamodb::types::AttributeValue;
use base64::Engine as _;
use clap::{Parser, Subcommand};
use serde_json::{Map, Value};

/// Repo root, resolved at compile time so the fixture path doesn't depend on cwd.
const REPO_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

#[derive(Parser)]
#[command(about = "Seed a local DynamoDB with test data")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write `local/seed/synthetic.json` into the local database. Needs no
    /// AWS access.
    Apply,
    /// Delete every row the running app itself writes — session tokens,
    /// WebAuthn state, submit codes/tokens — leaving the seeded fixture rows
    /// alone. Needs no AWS access.
    ///
    /// `apply` cannot do this: it only ever `put_item`s the fixture rows, so
    /// anything the app wrote on top (a passkey registered during a UI
    /// session, an expired login attempt) survives a reseed. `local-reset`
    /// fixes this too, by destroying and rebuilding every table, but that
    /// also throws away the seeded rows themselves.
    Clear,
}

// ── DynamoDB item <-> JSON ────────────────────────────────────────────────────
// The wire shape (`{"S": "x"}`) rather than anything friendlier, so a row
// round-trips through the fixture byte-for-byte — same convention as
// seslogin's local-seed.

fn json_to_av(value: &Value) -> Result<AttributeValue> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("attribute value must be an object, got {value}"))?;
    let (tag, v) = obj
        .iter()
        .next()
        .ok_or_else(|| anyhow!("attribute value object is empty"))?;
    let str_list = |v: &Value| -> Result<Vec<String>> {
        v.as_array()
            .ok_or_else(|| anyhow!("{tag} must be an array"))?
            .iter()
            .map(|e| {
                e.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("{tag} entries must be strings"))
            })
            .collect()
    };
    Ok(match tag.as_str() {
        "S" => AttributeValue::S(
            v.as_str()
                .ok_or_else(|| anyhow!("S must be a string"))?
                .to_string(),
        ),
        // Numbers stay strings on the wire; that is how DynamoDB avoids float loss.
        "N" => AttributeValue::N(
            v.as_str()
                .map(str::to_string)
                .or_else(|| v.as_i64().map(|n| n.to_string()))
                .ok_or_else(|| anyhow!("N must be a string or integer"))?,
        ),
        "BOOL" => AttributeValue::Bool(v.as_bool().ok_or_else(|| anyhow!("BOOL must be a bool"))?),
        "NULL" => AttributeValue::Null(true),
        "SS" => AttributeValue::Ss(str_list(v)?),
        "NS" => AttributeValue::Ns(str_list(v)?),
        "B" => AttributeValue::B(aws_sdk_dynamodb::primitives::Blob::new(
            base64::engine::general_purpose::STANDARD
                .decode(v.as_str().ok_or_else(|| anyhow!("B must be a string"))?)?,
        )),
        "L" => AttributeValue::L(
            v.as_array()
                .ok_or_else(|| anyhow!("L must be an array"))?
                .iter()
                .map(json_to_av)
                .collect::<Result<Vec<_>>>()?,
        ),
        "M" => AttributeValue::M(json_to_item(
            v.as_object()
                .ok_or_else(|| anyhow!("M must be an object"))?,
        )?),
        other => bail!("unsupported attribute tag {other:?} in fixture"),
    })
}

/// Keys starting with `_` are notes for whoever is reading the fixture, not
/// attributes — `synthetic.json` uses them to explain what each row is for.
fn json_to_item(obj: &Map<String, Value>) -> Result<HashMap<String, AttributeValue>> {
    obj.iter()
        .filter(|(k, _)| !k.starts_with('_'))
        .map(|(k, v)| Ok((k.clone(), json_to_av(v)?)))
        .collect()
}

// ── Apply ─────────────────────────────────────────────────────────────────────

fn seed_dir() -> PathBuf {
    Path::new(REPO_ROOT).join("local/seed")
}

fn load_tables(path: &Path) -> Result<Map<String, Value>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let doc: Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(doc
        .get("tables")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{} has no `tables` object", path.display()))?
        .clone())
}

/// Tables the running app writes into that no fixture owns. `apply`
/// overwrites anything seeded, so wiping these is what makes a re-seed
/// idempotent even after a UI session has been poking at the database.
///
/// **Deliberately excludes `user_token`**, even though the app writes to it
/// on every real login: `synthetic.json` seeds two specific rows there (the
/// owner/agent ready-made tokens `DEVELOPMENT.md` documents), so that table
/// *is* fixture-owned, unlike the others here. Wiping it on every `clear`
/// would destroy those documented tokens until the next `apply` — mirroring
/// seslogin's `local-seed.rs`, which excludes its own `user_token` table from
/// its transient list for the same reason. A real session token minted by
/// logging in during local UI work is left behind by `clear`, same as
/// seslogin's; only `local-reset` (which rebuilds every table) is guaranteed
/// to remove it.
///
/// `ticket`/`ticket_message`/`counter` are here too: once the running app
/// creates a ticket (e.g. `submitTicket` exercised from the UI), those rows
/// (and the per-instance counter it bumped) aren't owned by any fixture row,
/// so they'd otherwise survive a reseed and drift the counter out of step
/// with `synthetic.json`'s seeded ticket numbers.
const TRANSIENT_TABLES: &[&str] = &[
    "login_code",
    "webauthn_credential",
    "ephemeral_state",
    "processed_message",
    "ticket",
    "ticket_message",
    "counter",
];

async fn clear() -> Result<()> {
    let endpoint = microticket::local_dev::require_local_dynamodb_endpoint()?;
    let prefix = std::env::var("DB_PREFIX").map_err(|_| anyhow!("DB_PREFIX must be set"))?;
    let client = microticket::local_dev::dynamodb_client().await;
    println!("clearing {prefix}_* at {endpoint}");

    let mut total = 0usize;
    for table in TRANSIENT_TABLES {
        let name = format!("{prefix}_{table}");
        let hash_key = hash_key_for(table);
        let mut deleted = 0usize;
        let mut start_key = None;
        loop {
            let page = client
                .scan()
                .table_name(&name)
                .projection_expression(hash_key)
                .set_exclusive_start_key(start_key.clone())
                .send()
                .await
                .with_context(|| format!("scanning {name}"))?;
            for item in page.items() {
                let key = item
                    .get(hash_key)
                    .ok_or_else(|| anyhow!("{name}: a row has no `{hash_key}`"))?;
                client
                    .delete_item()
                    .table_name(&name)
                    .key(hash_key, key.clone())
                    .send()
                    .await
                    .with_context(|| format!("deleting from {name}"))?;
                deleted += 1;
            }
            start_key = page.last_evaluated_key().cloned();
            if start_key.is_none() {
                break;
            }
        }
        println!("    {:>20}: {} row(s) deleted", table, deleted);
        total += deleted;
    }
    println!("{total} row(s) deleted");
    Ok(())
}

/// Every `TRANSIENT_TABLES` entry today hash-keys on `id`; this exists so a
/// future addition (like `processed_message`, keyed on `ses_message_id`) is a
/// one-line change here rather than a wrong assumption baked into `clear`.
fn hash_key_for(table: &str) -> &'static str {
    match table {
        "processed_message" => "ses_message_id",
        _ => "id",
    }
}

async fn apply() -> Result<()> {
    let endpoint = microticket::local_dev::require_local_dynamodb_endpoint()?;
    let prefix = std::env::var("DB_PREFIX").map_err(|_| anyhow!("DB_PREFIX must be set"))?;
    let client = microticket::local_dev::dynamodb_client().await;
    println!("seeding {prefix}_* at {endpoint}");

    let path = seed_dir().join("synthetic.json");
    let tables = load_tables(&path)?;
    let mut written: Vec<(String, usize)> = Vec::new();
    for (table, rows) in &tables {
        let rows = rows
            .as_array()
            .ok_or_else(|| anyhow!("synthetic.json: {table} must be an array"))?;
        for row in rows {
            let obj = row
                .as_object()
                .ok_or_else(|| anyhow!("synthetic.json: {table} rows must be objects"))?;
            client
                .put_item()
                .table_name(format!("{prefix}_{table}"))
                .set_item(Some(json_to_item(obj)?))
                .send()
                .await
                .with_context(|| format!("writing a {table} row from synthetic.json"))?;
        }
        println!("    {:>15}: {} row(s)", table, rows.len());
        written.push((table.clone(), rows.len()));
    }
    let total: usize = written.iter().map(|(_, n)| n).sum();
    println!("{total} row(s) written");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    microticket::load_cli_env();
    tracing_subscriber::fmt::init();
    match Cli::parse().command {
        Command::Apply => apply().await,
        Command::Clear => clear().await,
    }
}
