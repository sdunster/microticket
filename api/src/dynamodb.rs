//! DynamoDB backend.
//!
//! This module holds the *generic* half only: connecting, naming tables, the
//! read-only guard, hydrating raw rows into typed records, and the batching/paging
//! machinery ([`Handler::get_records`], [`query_all`], [`scan_all`],
//! [`Handler::scan_page`]). Domain-specific query/write methods land on
//! [`crate::db::Handler`] and are implemented here as later steps add them.
//!
//! **House rule: omit optional attributes, never write `Null`.** An absent
//! attribute means "not set"; writing an explicit `Null` breaks sparse GSIs (an
//! attribute has to be *absent*, not null, for a row to drop out of a GSI that
//! projects it) and complicates hydration. Deleting or clearing an optional value
//! means removing the attribute (`REMOVE` in an update expression), not setting it
//! to null. See `CLAUDE.md`.

use crate::db::{self, HasID};
use crate::request_metrics::METRICS;
use anyhow::anyhow;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_dynamodb::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_dynamodb::operation::query::builders::QueryFluentBuilder;
use aws_sdk_dynamodb::operation::scan::builders::ScanFluentBuilder;
use aws_sdk_dynamodb::types::{ConsumedCapacity, KeysAndAttributes, ReturnConsumedCapacity};
use aws_sdk_dynamodb::{Client, types::AttributeValue};
use nanoid::nanoid;
use std::collections::HashMap;

const NANOID_ALPHABET: [char; 62] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I',
    'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'a', 'b',
    'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u',
    'v', 'w', 'x', 'y', 'z',
];

/// Extract the most useful info from a DynamoDB SdkError.
/// `{}` just prints "service error"; `{:?}` dumps raw HTTP responses.
/// This gives the DynamoDB error code + message for service errors, or the variant
/// name for infrastructure errors (dispatch failure, timeout, etc.).
fn sdk_err_msg<E: ProvideErrorMetadata>(e: SdkError<E>) -> String {
    match (e.code(), e.message()) {
        (Some(code), Some(msg)) => format!("{code}: {msg}"),
        (Some(code), None) => code.to_string(),
        (None, Some(msg)) => msg.to_string(),
        (None, None) => format!("{e}"),
    }
}

/// Generate a new unique ID for DB entities: a 12-char nanoid over a fixed
/// alphanumeric alphabet.
///
/// <https://alex7kom.github.io/nano-nanoid-cc/?alphabet=0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz&size=12&speed=1000&speedUnit=hour>
pub fn new_id() -> String {
    nanoid!(12, &NANOID_ALPHABET)
}

/// A raw DynamoDB row, with typed accessors that turn "wrong type" and
/// "missing/present" into `Result`/`Option` instead of panics.
#[derive(Clone, Debug, PartialEq)]
pub struct Item(HashMap<String, AttributeValue>);

impl Item {
    /// The item's primary key.
    ///
    /// A row with no `id`, or an `id` that is not a string, is corrupt rather than
    /// merely unexpected — but it is still just one bad row, so this reports a
    /// hydration error and leaves the rest of the table readable.
    pub fn id(&self) -> HydrationResult<String> {
        let value = self
            .0
            .get("id")
            .ok_or_else(|| anyhow!("Encountered an item with a missing id field"))?;
        let id = value
            .as_s()
            .map_err(|_| anyhow!("Encountered an item with an ID that is not a string"))?;
        Ok(id.to_string())
    }

    /// True if the attribute is present at all, regardless of type. Used for
    /// presence-marker attributes (like `ticket`'s `instance_visible`) that encode
    /// state by existence.
    pub fn has_field(&self, field: &str) -> bool {
        self.0.contains_key(field)
    }

    pub fn string_field(&self, field: &str) -> anyhow::Result<Option<String>> {
        if let Some(v) = self.0.get(field) {
            match v {
                AttributeValue::S(s) => Ok(Some(s.to_owned())),
                AttributeValue::Null(_) => Ok(None),
                _ => Err(anyhow!("Item had string field of wrong type: {}", field)),
            }
        } else {
            Ok(None)
        }
    }

    pub fn i64_field(&self, field: &str) -> anyhow::Result<Option<i64>> {
        if let Some(v) = self.0.get(field) {
            if let Ok(n) = v.as_n() {
                if let Ok(n) = n.parse::<i64>() {
                    Ok(Some(n))
                } else {
                    Err(anyhow!("Item had unparseable number field: {}", field))
                }
            } else {
                Err(anyhow!("Item had number field of wrong type: {}", field))
            }
        } else {
            Ok(None)
        }
    }

    pub fn bool_field(&self, field: &str) -> anyhow::Result<Option<bool>> {
        if let Some(v) = self.0.get(field) {
            if let Ok(b) = v.as_bool() {
                Ok(Some(*b))
            } else {
                Err(anyhow!("Item had bool field of wrong type: {}", field))
            }
        } else {
            Ok(None)
        }
    }

    /// Get a string set field, erroring if it is of the wrong type. If missing (or
    /// `Null`), returns an empty `Vec` — a String Set can't itself be empty in
    /// DynamoDB, so an empty set is always represented by the attribute's absence.
    pub fn string_set_field(&self, field: &str) -> anyhow::Result<Vec<String>> {
        if let Some(v) = self.0.get(field) {
            if let Ok(ss) = v.as_ss() {
                Ok(ss.to_owned())
            } else if v.as_null().is_ok() {
                Ok(vec![])
            } else {
                Err(anyhow!(
                    "Item had string set/null field of wrong type: {}",
                    field
                ))
            }
        } else {
            Ok(vec![])
        }
    }
}

/// A row that could not be turned into its typed record.
///
/// Carries the offending row's `id` where one could be read, because the whole
/// point of the error is to send someone to look at that row. Without it a failed
/// listing says only which table was being read — which, for a table with tens of
/// thousands of rows, is not an actionable report.
#[derive(Debug)]
pub struct HydrationError {
    source: anyhow::Error,
    record_id: Option<String>,
}

impl HydrationError {
    /// Attach the row's `id`, if it is not already known.
    ///
    /// An inner conversion may already have identified a more specific record, so
    /// an existing id is never overwritten.
    fn with_record_id(mut self, record_id: Option<String>) -> Self {
        if self.record_id.is_none() {
            self.record_id = record_id;
        }
        self
    }

    /// The offending row's `id`, where it was readable. `None` means the row's own
    /// `id` attribute is what is broken.
    pub fn record_id(&self) -> Option<&str> {
        self.record_id.as_deref()
    }
}

impl std::fmt::Display for HydrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.record_id {
            Some(id) => write!(f, "record {id}: {}", self.source),
            None => write!(f, "{}", self.source),
        }
    }
}

impl std::error::Error for HydrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Lets `TryInto` impls keep using `?` on `anyhow` errors. The id is filled in
/// afterwards by [`hydrate_item`], which is the only place that still has the raw
/// row.
impl From<anyhow::Error> for HydrationError {
    fn from(source: anyhow::Error) -> Self {
        Self {
            source,
            record_id: None,
        }
    }
}

pub type HydrationResult<T> = Result<T, HydrationError>;

/// Automatically convert hydration failures to `db::Error::Hydration`.
impl From<HydrationError> for db::Error {
    fn from(value: HydrationError) -> Self {
        db::Error::Hydration(value.to_string())
    }
}

#[derive(Debug)]
pub struct Handler {
    table_prefix: String,
    client: Client,
    read_only: bool,
}

impl Handler {
    pub fn table_name(&self, name: &str) -> String {
        format!("{}_{}", self.table_prefix, name)
    }

    pub async fn new(table_prefix: &str, read_only: bool) -> Self {
        let region_provider = RegionProviderChain::default_provider().or_else("ap-southeast-2");
        let config = crate::aws_config_loader()
            .region(region_provider)
            .load()
            .await;
        let client = Client::new(&config);
        Self {
            client,
            table_prefix: table_prefix.to_string(),
            read_only,
        }
    }

    /// Guard every write method calls first: this server was started without
    /// `--enable-mutations` (or the Lambda's `READ_ONLY` env var is set), so writes
    /// are refused rather than silently mutating whatever `DB_PREFIX` this points
    /// at.
    pub fn ensure_writable(&self) -> db::Result<()> {
        if self.read_only {
            Err(db::Error::MutationDisabled)
        } else {
            Ok(())
        }
    }

    /// One page of a base-table scan, hydrated leniently so a corrupt row is
    /// reported rather than failing the page. See [`db::ScanPage`] for why an empty
    /// page does not mean the walk is over.
    pub async fn scan_page<R>(
        &self,
        op: &'static str,
        name: &str,
        cursor: Option<db::ScanCursor>,
        limit: i32,
    ) -> db::Result<db::ScanPage<R>>
    where
        Item: TryInto<R, Error = HydrationError>,
    {
        let mut builder = self
            .client
            .scan()
            .table_name(self.table_name(name))
            .limit(limit)
            .return_consumed_capacity(ReturnConsumedCapacity::Total);
        if let Some(cursor) = cursor {
            builder = builder.set_exclusive_start_key(Some(HashMap::from([(
                "id".to_string(),
                AttributeValue::S(cursor.last_id),
            )])));
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| db::Error::Infrastructure(sdk_err_msg(e)))?;
        record_capacity(op, resp.consumed_capacity(), CapKind::Read);

        Ok(db::ScanPage {
            rows: hydrate_items_lenient(resp.items)
                .into_iter()
                .map(|row| row.map_err(db::Error::from))
                .collect(),
            next: scan_cursor_from_key(resp.last_evaluated_key),
        })
    }

    /// Batch-fetch rows by id, positionally aligned with `ids` (a `None` names
    /// exactly which requested id is missing).
    ///
    /// Chunks into groups of 100 (DynamoDB's `BatchGetItem` limit) and retries
    /// `UnprocessedKeys` with backoff — DynamoDB is allowed to return fewer items
    /// than requested (under throttling, or when a response would exceed 16MB) and
    /// defers the rest there; skipping the retry would make an existing row look
    /// like it doesn't exist.
    pub async fn get_records<R, T>(&self, name: &str, ids: &[T]) -> db::Result<Vec<Option<R>>>
    where
        T: AsRef<str> + Sync,
        R: HasID + 'static,
        Item: TryInto<R, Error = HydrationError>,
    {
        let ids = ids.iter().map(|id| id.as_ref()).collect::<Vec<&str>>();
        let table_name = self.table_name(name);
        let mut results: HashMap<String, R> = HashMap::new();

        for chunk in ids.chunks(100) {
            let mut pending: Vec<HashMap<String, AttributeValue>> = chunk
                .iter()
                .map(|id| HashMap::from([("id".to_string(), AttributeValue::S(id.to_string()))]))
                .collect();

            for attempt in 1..=BATCH_GET_MAX_ATTEMPTS {
                if attempt > 1 {
                    tokio::time::sleep(batch_get_backoff(attempt - 1)).await;
                }

                let resp = self
                    .client
                    .batch_get_item()
                    .request_items(
                        table_name.clone(),
                        KeysAndAttributes::builder()
                            .set_keys(Some(pending.clone()))
                            .build()
                            .map_err(|e| db::Error::Infrastructure(e.to_string()))?,
                    )
                    .return_consumed_capacity(ReturnConsumedCapacity::Total)
                    .send()
                    .await
                    .map_err(|e| db::Error::Infrastructure(sdk_err_msg(e)))?;

                batch_record_capacity(
                    &format!("batch_get {name}"),
                    resp.consumed_capacity(),
                    CapKind::Read,
                );
                if let Some(mut responses) = resp.responses
                    && let Some(items) = responses.remove(&table_name)
                {
                    for item in items {
                        let rec: R = hydrate_item(item)?;
                        results.insert(rec.id().to_string(), rec);
                    }
                }

                pending = unprocessed_keys_for(&table_name, resp.unprocessed_keys);
                if pending.is_empty() {
                    break;
                }
                tracing::warn!(
                    table = %table_name,
                    unprocessed = pending.len(),
                    attempt,
                    "batch_get returned unprocessed keys; retrying"
                );
            }

            if !pending.is_empty() {
                // Returning `None` for these would be indistinguishable from the
                // rows not existing, so fail loudly instead.
                return Err(db::Error::Infrastructure(format!(
                    "batch_get {table_name}: {} key(s) still unprocessed after {BATCH_GET_MAX_ATTEMPTS} attempts",
                    pending.len(),
                )));
            }
        }

        Ok(ids
            .clone()
            .into_iter()
            .map(|id| results.remove(id))
            .collect())
    }
}

/// Read a scan's continuation key back into a cursor.
///
/// A base-table scan's `LastEvaluatedKey` is just the primary key, and every
/// scannable table is hash-keyed on a string `id`. A key of any other shape means
/// the assumption no longer holds, so treat it as the end of the walk rather than
/// guessing.
fn scan_cursor_from_key(key: Option<HashMap<String, AttributeValue>>) -> Option<db::ScanCursor> {
    key?.get("id")
        .and_then(|v| v.as_s().ok())
        .map(|id| db::ScanCursor {
            last_id: id.to_string(),
        })
}

/// How many times a single `BatchGetItem` chunk is sent before giving up. Four
/// retries after the first attempt, which at the backoff below spans roughly
/// 750ms — long enough to ride out an ordinary throttle, short enough not to stall
/// a request.
const BATCH_GET_MAX_ATTEMPTS: usize = 5;

/// Delay before retry number `retry` (1-based): 50ms doubling to a 1s ceiling.
fn batch_get_backoff(retry: usize) -> std::time::Duration {
    let ms = 50u64
        .saturating_mul(1u64 << retry.min(8).saturating_sub(1))
        .min(1000);
    std::time::Duration::from_millis(ms)
}

/// The keys DynamoDB deferred for our table, if any.
///
/// `UnprocessedKeys` is keyed by table name and is absent — not empty — when
/// everything was processed, so both shapes have to mean "nothing left to do".
fn unprocessed_keys_for(
    table_name: &str,
    unprocessed: Option<HashMap<String, KeysAndAttributes>>,
) -> Vec<HashMap<String, AttributeValue>> {
    unprocessed
        .and_then(|mut tables| tables.remove(table_name))
        .map(|ka| ka.keys)
        .unwrap_or_default()
}

enum CapKind {
    Read,
    /// Not yet constructed anywhere — there are no write methods until step 3.
    #[allow(dead_code)]
    Write,
}

fn record_capacity(desc: &str, cap: Option<&ConsumedCapacity>, kind: CapKind) {
    let rcu = cap.and_then(|c| c.read_capacity_units());
    let wcu = cap.and_then(|c| c.write_capacity_units());
    let (rcu, wcu) = if rcu.is_some() || wcu.is_some() {
        (rcu.unwrap_or(0.0), wcu.unwrap_or(0.0))
    } else {
        let total = cap.and_then(|c| c.capacity_units()).unwrap_or(0.0);
        match kind {
            CapKind::Read => (total, 0.0),
            CapKind::Write => (0.0, total),
        }
    };
    let _ = METRICS.try_with(|m| m.record(desc, rcu, wcu));
}

fn batch_record_capacity(desc: &str, caps: &[ConsumedCapacity], kind: CapKind) {
    let rcu_sum: f64 = caps.iter().filter_map(|c| c.read_capacity_units()).sum();
    let wcu_sum: f64 = caps.iter().filter_map(|c| c.write_capacity_units()).sum();
    let (rcu, wcu) = if rcu_sum > 0.0 || wcu_sum > 0.0 {
        (rcu_sum, wcu_sum)
    } else {
        let total: f64 = caps.iter().filter_map(|c| c.capacity_units()).sum();
        match kind {
            CapKind::Read => (total, 0.0),
            CapKind::Write => (0.0, total),
        }
    };
    let _ = METRICS.try_with(|m| m.record(desc, rcu, wcu));
}

/// Hydrate one raw row, tagging any failure with that row's `id`.
///
/// The id is read before the conversion consumes the item, so a row that fails on
/// some *other* attribute can still be named. A row whose `id` is itself unreadable
/// reports no id — the error message then describes the `id` problem directly.
fn hydrate_item<T>(raw: HashMap<String, AttributeValue>) -> HydrationResult<T>
where
    Item: TryInto<T, Error = HydrationError>,
{
    let item = Item(raw);
    let record_id = item.id().ok();
    item.try_into().map_err(|e| e.with_record_id(record_id))
}

/// Hydrate a page of rows, stopping at the first bad one.
///
/// This is the default because production read paths should fail loudly rather
/// than quietly serve a short list. Callers that need to survey every bad row in
/// one pass want [`hydrate_items_lenient`] instead.
fn hydrate_items<T>(items: Option<Vec<HashMap<String, AttributeValue>>>) -> HydrationResult<Vec<T>>
where
    Item: TryInto<T, Error = HydrationError>,
{
    items
        .unwrap_or_default()
        .into_iter()
        .map(hydrate_item)
        .collect()
}

/// Hydrate a page of rows, reporting each row's outcome independently.
///
/// One corrupt row hides every row after it under [`hydrate_items`]. Here the
/// caller sees every failure at once, each already tagged with its record id.
pub fn hydrate_items_lenient<T>(
    items: Option<Vec<HashMap<String, AttributeValue>>>,
) -> Vec<HydrationResult<T>>
where
    Item: TryInto<T, Error = HydrationError>,
{
    items
        .unwrap_or_default()
        .into_iter()
        .map(hydrate_item)
        .collect()
}

/// Run a query to exhaustion, following DynamoDB's 1MB-per-page continuation key.
///
/// `build` is invoked once per page because the SDK's fluent builders are not
/// `Clone`; it must produce an identically-configured request every time.
/// `ExclusiveStartKey` and `ReturnConsumedCapacity` are applied by this helper, and
/// capacity is recorded per page so the metrics reflect the real cost of the full
/// walk.
///
/// The loop breaks only when DynamoDB reports no continuation key. With a
/// `FilterExpression` a page can come back with zero items and still have more to
/// read, so breaking on an empty item list would silently truncate.
pub async fn query_all_items(
    op: &'static str,
    build: impl Fn() -> QueryFluentBuilder,
) -> db::Result<Vec<HashMap<String, AttributeValue>>> {
    let mut items: Vec<HashMap<String, AttributeValue>> = Vec::new();
    let mut exclusive_start_key: Option<HashMap<String, AttributeValue>> = None;

    loop {
        let mut builder = build().return_consumed_capacity(ReturnConsumedCapacity::Total);
        if let Some(esk) = exclusive_start_key.take() {
            builder = builder.set_exclusive_start_key(Some(esk));
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| db::Error::Infrastructure(sdk_err_msg(e)))?;
        record_capacity(op, resp.consumed_capacity(), CapKind::Read);

        items.extend(resp.items.unwrap_or_default());
        exclusive_start_key = resp.last_evaluated_key;
        if exclusive_start_key.is_none() {
            return Ok(items);
        }
    }
}

/// [`query_all_items`], hydrated into typed records.
pub async fn query_all<T>(
    op: &'static str,
    build: impl Fn() -> QueryFluentBuilder,
) -> db::Result<Vec<T>>
where
    Item: TryInto<T, Error = HydrationError>,
{
    Ok(hydrate_items(Some(query_all_items(op, build).await?))?)
}

/// Scan equivalent of [`query_all_items`]. Same 1MB paging rules apply.
pub async fn scan_all_items(
    op: &'static str,
    build: impl Fn() -> ScanFluentBuilder,
) -> db::Result<Vec<HashMap<String, AttributeValue>>> {
    let mut items: Vec<HashMap<String, AttributeValue>> = Vec::new();
    let mut exclusive_start_key: Option<HashMap<String, AttributeValue>> = None;

    loop {
        let mut builder = build().return_consumed_capacity(ReturnConsumedCapacity::Total);
        if let Some(esk) = exclusive_start_key.take() {
            builder = builder.set_exclusive_start_key(Some(esk));
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| db::Error::Infrastructure(sdk_err_msg(e)))?;
        record_capacity(op, resp.consumed_capacity(), CapKind::Read);

        items.extend(resp.items.unwrap_or_default());
        exclusive_start_key = resp.last_evaluated_key;
        if exclusive_start_key.is_none() {
            return Ok(items);
        }
    }
}

/// [`scan_all_items`], hydrated into typed records.
pub async fn scan_all<T>(
    op: &'static str,
    build: impl Fn() -> ScanFluentBuilder,
) -> db::Result<Vec<T>>
where
    Item: TryInto<T, Error = HydrationError>,
{
    Ok(hydrate_items(Some(scan_all_items(op, build).await?))?)
}

impl db::Handler for Handler {}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(fields: Vec<(&str, AttributeValue)>) -> Item {
        Item(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    #[test]
    fn new_id_is_a_12_char_nanoid() {
        let id = new_id();
        assert_eq!(id.len(), 12);
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(new_id(), new_id());
    }

    #[test]
    fn id_reads_the_id_field() {
        let i = item(vec![("id", AttributeValue::S("abc123".into()))]);
        assert_eq!(i.id().unwrap(), "abc123");
    }

    #[test]
    fn id_errors_when_missing() {
        let i = item(vec![]);
        assert!(i.id().is_err());
    }

    #[test]
    fn id_errors_when_not_a_string() {
        let i = item(vec![("id", AttributeValue::N("1".into()))]);
        assert!(i.id().is_err());
    }

    #[test]
    fn string_field_reads_present_and_missing() {
        let i = item(vec![("name", AttributeValue::S("hi".into()))]);
        assert_eq!(i.string_field("name").unwrap(), Some("hi".to_string()));
        assert_eq!(i.string_field("missing").unwrap(), None);
    }

    #[test]
    fn string_field_errors_on_wrong_type() {
        let i = item(vec![("name", AttributeValue::N("1".into()))]);
        assert!(i.string_field("name").is_err());
    }

    #[test]
    fn i64_field_reads_present_and_missing() {
        let i = item(vec![("n", AttributeValue::N("42".into()))]);
        assert_eq!(i.i64_field("n").unwrap(), Some(42));
        assert_eq!(i.i64_field("missing").unwrap(), None);
    }

    #[test]
    fn bool_field_reads_present_and_missing() {
        let i = item(vec![("b", AttributeValue::Bool(true))]);
        assert_eq!(i.bool_field("b").unwrap(), Some(true));
        assert_eq!(i.bool_field("missing").unwrap(), None);
    }

    #[test]
    fn string_set_field_missing_is_empty() {
        let i = item(vec![]);
        assert_eq!(i.string_set_field("tags").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn string_set_field_reads_the_set() {
        let i = item(vec![(
            "tags",
            AttributeValue::Ss(vec!["a".into(), "b".into()]),
        )]);
        assert_eq!(i.string_set_field("tags").unwrap(), vec!["a", "b"]);
    }

    #[test]
    fn has_field_is_true_regardless_of_type() {
        let i = item(vec![("marker", AttributeValue::N("1".into()))]);
        assert!(i.has_field("marker"));
        assert!(!i.has_field("absent"));
    }

    /// A tiny hydration target, local to the tests, exercising [`hydrate_item`] and
    /// [`hydrate_items_lenient`]'s per-row id tagging.
    #[derive(Debug, PartialEq)]
    struct TestRow {
        id: String,
        name: String,
    }

    impl HasID for TestRow {
        fn id(&self) -> &str {
            &self.id
        }
    }

    impl TryInto<TestRow> for Item {
        type Error = HydrationError;
        fn try_into(self) -> Result<TestRow, Self::Error> {
            Ok(TestRow {
                id: self.id()?,
                name: self
                    .string_field("name")?
                    .ok_or_else(|| anyhow!("TestRow missing name"))?,
            })
        }
    }

    #[test]
    fn hydrate_items_lenient_reports_each_row_independently() {
        let rows = vec![
            HashMap::from([
                ("id".to_string(), AttributeValue::S("r1".into())),
                ("name".to_string(), AttributeValue::S("Row One".into())),
            ]),
            // Missing `name` — should fail, tagged with its id.
            HashMap::from([("id".to_string(), AttributeValue::S("r2".into()))]),
        ];
        let results: Vec<HydrationResult<TestRow>> = hydrate_items_lenient(Some(rows));
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].as_ref().unwrap(),
            &TestRow {
                id: "r1".into(),
                name: "Row One".into()
            }
        );
        let err = results[1].as_ref().unwrap_err();
        assert_eq!(err.record_id(), Some("r2"));
    }

    #[test]
    fn hydrate_items_lenient_of_empty_input_is_empty() {
        let results: Vec<HydrationResult<TestRow>> = hydrate_items_lenient(None);
        assert!(results.is_empty());
    }
}
