# Database Schema Reference

microticket uses DynamoDB as its database backend. All tables are defined in `infra/dynamodb.tf` —
that file, not this one, is the source of truth for the deployed tables; `api/src/bin/local-tables.rs`
is transcribed from it by hand for local development and must be kept in sync (`make
local-tables-check` in CI is the tripwire). This project is prod-only, so unlike some sibling
projects there is no parallel test-prefix table set: every table here is named
`{DB_PREFIX}_{entity}`, and `DB_PREFIX` is `prod` in the deployed stack, `local` in local
development.

All tables use `PAY_PER_REQUEST` billing (on-demand capacity); there is no provisioned throughput
to tune. Hash keys are a 12-char nanoid unless noted otherwise (a few tables — `inbound_address`,
`login_code`, `processed_message` — key on a natural value instead, called out below).

DynamoDB only enforces uniqueness on the primary key. All other uniqueness requirements (e.g. one
`slug` per instance) are enforced at the application layer.

In DynamoDB, only attributes that are part of a table key or a GSI key must be declared in the
table definition. All other fields are schema-free per item — they exist because application code
writes them, not because DynamoDB requires them.

All IDs are exposed to the API layer as opaque strings. Conversion/validation happens inside
`dynamodb.rs`, never in callers.

**House rule (see `CLAUDE.md`): optional attributes are omitted, never written as `Null`.**
Clearing an optional value means removing the attribute (`REMOVE` in an update expression), not
setting it to null — this matters most for the sparse-GSI marker attributes below, where an
absent attribute is what drops a row out of an index.

---

## Schema

### `{prefix}_instance`

| Attribute | Type | Role                  |
| --------- | ---- | --------------------- |
| `id`      | S    | Hash key (PK) — nanoid |
| `slug`    | S    | GSI hash key           |

**GSIs:**

| GSI           | Hash key | Sort key | Projection | Purpose                                                                          |
| ------------- | -------- | -------- | ---------- | --------------------------------------------------------------------------------- |
| `slug-index`  | `slug`   | —        | KEYS_ONLY  | Resolve a URL slug (`/app/:slug`, `/submit/:slug`) to an instance id             |

`KEYS_ONLY` is deliberately minimal: the resolved id drives a separate `GetItem` for the full
instance record, so projecting more into the index would just be wasted storage.

**Non-obvious attributes (not in the table definition):**

- `name` (S) — display name
- `from_name` (S) — the `From:` display name used on outbound mail
- `signature` (S) — appended to outbound replies
- `public_submission_enabled` (Bool) — opt-in, default off/absent; gates whether the instance
  appears in the bare `/submit` list
- `deleted` (Bool)

---

### `{prefix}_inbound_address`

| Attribute     | Type | Role                                              |
| ------------- | ---- | -------------------------------------------------- |
| `address`     | S    | Hash key (PK) — lowercased full address or `*@domain` wildcard |
| `instance_id` | S    | GSI hash key                                        |

The hash key is a natural value, not an opaque id: inbound-mail routing does a direct `GetItem`
on the lowercased, `+tag`-stripped recipient address, then falls back to `*@domain`. Making the
address itself the key means routing is a `GetItem`, not a `Query`.

**GSIs:**

| GSI                 | Hash key      | Sort key | Projection | Purpose                                                        |
| ------------------- | ------------- | -------- | ---------- | ---------------------------------------------------------------- |
| `instance_id-index` | `instance_id` | —        | ALL        | Instance settings page: list every address owned by an instance |

`ALL`: the settings UI renders `address`/`kind`/`created_at` directly from the list, and an
instance has at most a handful of addresses, so the per-item storage cost of projecting
everything is negligible next to avoiding an N-way `BatchGetItem`.

**Non-obvious attributes:**

- `kind` (S) — e.g. `exact` or `wildcard`
- `created_at` (N) — Unix timestamp

---

### `{prefix}_user`

| Attribute | Type | Role                  |
| --------- | ---- | --------------------- |
| `id`      | S    | Hash key (PK) — nanoid |
| `email`   | S    | GSI hash key           |

**GSIs:**

| GSI            | Hash key | Sort key | Projection | Purpose                                                                    |
| -------------- | -------- | -------- | ---------- | ---------------------------------------------------------------------------- |
| `email-index`  | `email`  | —        | KEYS_ONLY  | Email-code login (`requestAuthCode`/`verifyAuthCode`): resolve email → user id |

`KEYS_ONLY` for the same reason as `instance.slug-index` — the login path only needs the id to
drive the next `GetItem`.

**Non-obvious attributes:**

- `name` (S)
- `enabled` (Bool)

---

### `{prefix}_membership`

| Attribute     | Type | Role                  |
| ------------- | ---- | --------------------- |
| `id`          | S    | Hash key (PK) — nanoid |
| `instance_id` | S    | GSI hash key           |
| `user_id`     | S    | GSI hash key           |

**GSIs:**

| GSI                 | Hash key      | Sort key | Projection | Purpose                                                        |
| ------------------- | ------------- | -------- | ---------- | ---------------------------------------------------------------- |
| `instance_id-index` | `instance_id` | —        | ALL        | Instance settings page: list every member (and role) of an instance |
| `user_id-index`     | `user_id`     | —        | ALL        | `me` query / instance switcher: list every instance a user belongs to |

`ALL` on both: a user typically belongs to a handful of instances, and an instance to a handful
of members, so rendering either list directly from the GSI beats an N-way `BatchGetItem`.

**Non-obvious attributes:**

- `role` (S) — `owner` \| `agent`

---

### `{prefix}_ticket`

| Attribute            | Type | Role                                                                                   |
| -------------------- | ---- | --------------------------------------------------------------------------------------- |
| `id`                 | S    | Hash key (PK) — nanoid                                                                   |
| `instance_status`    | S    | GSI hash key — `"{instance_id}#open"` \| `"{instance_id}#closed"` \| `"{instance_id}#deleted"`, always present |
| `instance_visible`   | S    | Sparse GSI hash key — `"{instance_id}"`, present only when status != `deleted`          |
| `instance_assignee`  | S    | Sparse GSI hash key — `"{instance_id}#{assignee_user_id}"`, present only when assigned and visible |
| `last_activity_at`   | N    | GSI sort key (all three listing GSIs)                                                    |
| `instance_number`    | S    | GSI hash key — `"{instance_id}#{number}"`, always present                                |

**GSIs:**

| GSI                                          | Hash key             | Sort key            | Projection | Purpose                                                     |
| --------------------------------------------- | --------------------- | -------------------- | ---------- | -------------------------------------------------------------- |
| `instance_status-last_activity_at-index`      | `instance_status`     | `last_activity_at`   | ALL        | Open and Closed list pages, newest activity first             |
| `instance_visible-last_activity_at-index`     | `instance_visible`    | `last_activity_at`   | ALL        | All list page (every non-deleted ticket)                      |
| `instance_assignee-last_activity_at-index`    | `instance_assignee`   | `last_activity_at`   | ALL        | "Assigned to me" filter                                        |
| `instance_number-index`                       | `instance_number`     | —                    | KEYS_ONLY  | Subject-tag resolution (`[#{slug}-{number}]`) during inbound-mail threading |

**The three composite marker attributes — `instance_status`, `instance_visible`,
`instance_assignee` — are the whole listing story**, and per the house rule (omit, never
`Null`) are written and `REMOVE`d, never nulled. DynamoDB only indexes an item into a GSI when
the GSI's hash-key attribute is *present* on that item; writing an explicit `Null` would leave the
item indexed (with a null sort position) rather than dropping it out. Deleting a ticket removes
`instance_visible` and `instance_assignee` and flips `instance_status` to `…#deleted` — one write
drops the ticket out of every normal list while it stays queryable by id and by an explicit
`status: DELETED` filter.

`ALL` projection on the three listing GSIs: every ticket list page in the web UI renders
subject/status/requester/assignee/`last_activity_at` straight from the list response, so
projecting everything avoids a per-row `GetItem` for what is, by construction, the common case
(paging through a queue). `KEYS_ONLY` on `instance_number-index`: it is a single exact-match
lookup during inbound routing, and the caller does a follow-up strongly-consistent `GetItem`
before mutating the ticket anyway (GSI reads are only eventually consistent), so nothing is
gained by projecting more.

**Non-obvious attributes:**

- `instance_id` (S)
- `number` (N) — per-instance sequential ticket number; the human-visible part of
  `instance_number` and of the `[#{slug}-{number}]` subject tag
- `subject` (S)
- `status` (S) — `open` \| `closed` \| `deleted`; the un-composited form of `instance_status`'s
  suffix, kept alongside it because resolvers read `status` directly far more often than they
  need the composite
- `requester_emails` (SS) — string set
- `cc_emails` (SS) — string set
- `assignee_user_id` (S) — absent when unassigned
- `reply_token` (S) — opaque 16-char token embedded in `Reply-To` as `+t{reply_token}` for
  threading
- `created_at` (N)
- `updated_at` (N)

---

### `{prefix}_ticket_message`

| Attribute        | Type | Role                  |
| ---------------- | ---- | --------------------- |
| `id`             | S    | Hash key (PK) — nanoid |
| `ticket_id`      | S    | GSI hash key           |
| `created_at`     | N    | GSI sort key           |
| `rfc_message_id` | S    | GSI hash key           |

**GSIs:**

| GSI                             | Hash key         | Sort key     | Projection | Purpose                                                                |
| -------------------------------- | ----------------- | ------------- | ---------- | -------------------------------------------------------------------------- |
| `ticket_id-created_at-index`     | `ticket_id`       | `created_at`  | ALL        | Thread view: every message for a ticket, in order                        |
| `rfc_message_id-index`           | `rfc_message_id`  | —             | KEYS_ONLY  | Inbound-mail threading: resolve `In-Reply-To`/`References` to the message (and ticket) it answers |

`ALL` on `ticket_id-created_at-index`: the thread view renders `body_text`/`body_html`/
`attachments` directly from the list, and a ticket's message count is small enough that
projecting everything beats an N-way `GetItem` per page render. `KEYS_ONLY` on
`rfc_message_id-index`: this is an id-resolution step during inbound routing; the caller reads
the resolved message (and its parent ticket) with a separate strongly-consistent `GetItem` before
appending anything.

**Non-obvious attributes:**

- `kind` (S) — `inbound` \| `reply` \| `note` \| `system`. `note` rows are internal-only and
  filtered out of any requester-visible resolver — they are never emailed
- `author_user_id` (S) — present for `reply`/`note`; absent for `inbound`
- `from_email` (S) — present for `inbound`; absent otherwise
- `to_emails` / `cc_emails` (SS) — snapshot of the envelope at send/receive time, independent of
  the ticket's current requester/CC lists (which can change afterward)
- `body_text` (S), `body_html` (S) — absent if the message had no such part
- `in_reply_to` (S), `references` (S) — absent for the first message on a ticket
- `attachments` (list of `{s3_key, filename, content_type, size}`) — absent when empty
- `raw_s3_key` (S) — present only on `inbound` rows, pointing at the raw MIME in S3

---

### `{prefix}_counter`

| Attribute | Type | Role                                          |
| --------- | ---- | ---------------------------------------------- |
| `id`      | S    | Hash key (PK) — the instance id, not a nanoid  |

No GSIs. One row per instance; the row's `id` is the owning instance's id directly (not a
separately generated nanoid), since the counter's whole purpose is a 1:1 relationship with an
instance and there is no other access pattern to support.

**Non-obvious attributes:**

- `next_ticket_number` (N) — incremented with an atomic `UpdateItem ADD`, never read-then-written;
  see "Known issues and risks" below for what happens if that rule is broken

---

### `{prefix}_login_code`

| Attribute | Type | Role                             |
| --------- | ---- | ----------------------------------- |
| `email`   | S    | Hash key (PK) — the address itself |

No GSIs. Ephemeral: `ttl { attribute_name = "expires_at" }`, no deletion protection, no PITR — a
login code is worthless once expired, so there is nothing to protect.

**Non-obvious attributes:**

- `code_hash` (S) — sha256 of the 6-digit code, never the code itself
- `expires_at` (N) — 10 minutes from issuance; drives DynamoDB TTL deletion
- `attempts` (N) — incremented per failed `verifyAuthCode`; burned after 5
- `last_sent_at` (N) — enforces the one-send-per-30s rate limit

---

### `{prefix}_user_token`

| Attribute    | Type | Role                  |
| ------------ | ---- | --------------------- |
| `id`         | S    | Hash key (PK) — nanoid |
| `token_hash` | S    | GSI hash key           |

**GSIs:**

| GSI                 | Hash key      | Sort key | Projection | Purpose                                                           |
| ------------------- | ------------- | -------- | ---------- | -------------------------------------------------------------------- |
| `token_hash-index`  | `token_hash`  | —        | KEYS_ONLY  | Used on every authenticated request that presents an `mtu_` bearer token |

`KEYS_ONLY`: the resolved id drives a follow-up `GetItem` for the rest of the token record
(`user_id`, expiry) — this table is on the hot path of every authenticated request, so keeping
the index small matters more here than on most tables.

**Non-obvious attributes:**

- `user_id` (S)
- `token_hash` (S) — sha256 of the opaque `mtu_`-prefixed token; the token itself is never stored
- `expires_at` (N) — 8 hours from issuance. Not the table's TTL attribute — `user_token` is a
  durable table with deletion protection + PITR, not one of the three ephemeral tables, so expiry
  is enforced by application-layer comparison against `expires_at`, not by DynamoDB TTL

---

### `{prefix}_webauthn_credential`

| Attribute | Type | Role                  |
| --------- | ---- | --------------------- |
| `id`      | S    | Hash key (PK) — nanoid |
| `user_id` | S    | GSI hash key           |

**GSIs:**

| GSI               | Hash key  | Sort key | Projection | Purpose                                                          |
| ----------------- | --------- | -------- | ---------- | -------------------------------------------------------------------- |
| `user_id-index`   | `user_id` | —        | ALL        | Passkey login (discoverable-credential flow) and the settings page's passkey list |

`ALL`: both call sites need the full serialized credential (not just its id) — the login flow to
verify a signature, the settings page to render each passkey's metadata.

**Non-obvious attributes:**

- `passkey` (S) — the serialized `webauthn-rs` `Passkey`, opaque to everything except the
  `webauthn-rs` crate
- `name` (S) — user-supplied label, shown in the settings page
- `created_at` (N)
- `last_used_at` (N) — absent until first use

---

### `{prefix}_ephemeral_state`

| Attribute | Type | Role                  |
| --------- | ---- | --------------------- |
| `id`      | S    | Hash key (PK) — nanoid |

No GSIs. Ephemeral: `ttl { attribute_name = "expires_at" }`, no deletion protection, no PITR.
Generic key/value store, namespaced by a `kind` discriminator that is deliberately *not* a GSI
key — every access pattern here is a `GetItem` by `id` (the opaque token/challenge handed to the
client), never a scan or query by kind. Backs:

- WebAuthn registration/login challenges (`kind: "reg"` / `"auth"`)
- The requester submit-token flow's short-lived, single-purpose capability token (`kind:
  "submit_token"`), scoped to `{email, instance_id}` and only usable for `submitTicket`

**Non-obvious attributes:**

- `kind` (S) — discriminator; see above
- `payload` (S) — opaque JSON, meaning depends on `kind`
- `expires_at` (N) — drives TTL deletion

---

### `{prefix}_processed_message`

| Attribute        | Type | Role                                         |
| ----------------- | ---- | ----------------------------------------------- |
| `ses_message_id`  | S    | Hash key (PK) — SES's own message id, not a nanoid |

No GSIs. Ephemeral: `ttl { attribute_name = "expires_at" }`, no deletion protection, no PITR.
Inbound-mail idempotency: one row per SES message id, written with a conditional `PutItem`
(`attribute_not_exists(ses_message_id)`) *before* any other processing — a duplicate SQS
delivery of the same message finds the condition already failed and exits cleanly instead of
creating a second ticket or reply. See "Known issues and risks" below for the window this does
and does not close.

**Non-obvious attributes:**

- `processed_at` (N)
- `expires_at` (N) — drives TTL deletion; retained only long enough to outlast SQS's redelivery
  window (the queue's visibility timeout and its 3-retry DLQ policy), not indefinitely

---

## Known issues and risks

This section covers correctness bugs, race conditions, and consistency gaps that follow from the
design above, called out ahead of the code that would trigger them landing (steps 5–7 of the
build plan). Update it as steps land, rather than leaving it purely speculative.

---

### Race conditions

#### `counter` — the per-instance ticket number must come from an atomic `ADD`, never read-then-write

The obvious-looking implementation — `GetItem` the counter row, add 1 in application code,
`PutItem` the result back — has an unavoidable race: two inbound-mail Lambda invocations
processing two new messages for the same instance at (nearly) the same time can both read
`next_ticket_number = 41`, both compute 42, and both write 42. The result is two tickets sharing
one `[#{slug}-42]` subject tag and one `instance_number` GSI value — which breaks
`instance_number-index` lookups (an exact-match query is supposed to resolve to *one* ticket) and
produces a confusing subject collision for the two requesters involved.

The fix is what this table exists for: `UpdateItem` with `ADD next_ticket_number :one` and
`ReturnValues::UpdatedNew`, which DynamoDB executes as a single atomic read-modify-write
server-side — no two concurrent `ADD`s can observe the same "before" value. The counter table has
no other access pattern, so there is no legitimate reason for a resolver to `GetItem` it directly;
if one ever does, that is the bug to look for first.

#### `processed_message` — the idempotency check has a window, not a guarantee, against non-SQS redelivery

The conditional `PutItem` (`attribute_not_exists(ses_message_id)`) closes the specific race SQS
redelivery creates: two overlapping Lambda invocations for the same `ses_message_id` (an
at-least-once queue redelivering before the first invocation finishes, or two invocations from a
raised `batch_size`) will have exactly one `PutItem` succeed, and the loser exits without touching
`ticket`/`ticket_message`. That is a real, load-bearing guarantee, not a false one.

What it does *not* guard against: the row is written before step 6 ("Store") completes, so a
Lambda that crashes (times out, OOMs, is killed) *after* the `PutItem` succeeds but *before* the
`ticket_message` is stored will leave `processed_message` marked as processed for a message that
was, from the requester's point of view, silently dropped. SQS's retry will not help — the
idempotency check itself will reject the retry. This is the inverse failure mode from the one the
table is designed to prevent, and is an inherent tension in "mark done first, then do the work":
marking done *after* the work closes this gap but reopens the original race (a second delivery
arriving mid-processing would not yet see the row). A durable fix needs a two-phase state (e.g. a
`processing` marker with its own short TTL, promoted to permanent only after the ticket write
commits, with a periodic sweep re-driving anything stuck in `processing` past its TTL) — not
implemented as of this step; flagged here so step 7 (inbound mail) makes a deliberate choice
about it rather than inheriting the simple version by default.

#### `ticket` marker attributes — a crash between two GSI-affecting updates leaves an inconsistent listing state

Setting `instance_status`, `instance_visible` and `instance_assignee` correctly together (e.g. on
delete: flip `instance_status` to `…#deleted`, `REMOVE instance_visible`, `REMOVE
instance_assignee`) needs to happen in a single `UpdateItem` call so it is atomic from any
reader's point of view — DynamoDB does not offer a multi-attribute "all or nothing across two
calls" primitive short of a transaction. As long as every write path that touches more than one
of these three attributes does so in one `UpdateItem`, this is not a real gap (DynamoDB guarantees
atomicity within a single item's `UpdateItem`). It is called out here as a *discipline* risk
rather than a DynamoDB limitation: a future write path that updates `instance_status` in one call
and `instance_visible` in a second, for expedience, would reintroduce exactly this race under a
crash between the two calls. Code review on any new ticket-mutating resolver should check this.

---

### GSI eventual consistency

All GSI-based lookups use DynamoDB's default eventually-consistent reads; strong consistency is
not available on GSIs. This is expected to affect, once the relevant steps land:

- **`instance.slug-index`** — an instance created and immediately visited at `/app/:slug` (e.g.
  scripted setup) could momentarily 404.
- **`user.email-index`** — a user created and immediately signing in could momentarily fail to
  resolve. Low risk in practice since account creation and first login are rarely within
  milliseconds of each other.
- **`ticket.instance_number-index`** — a ticket created moments before an inbound reply with a
  matching subject tag arrives could fall through subject-tag resolution to "no match", opening a
  new ticket instead of appending to the existing one. The fallback (new ticket) is not silently
  wrong — the reply is stored, just under the wrong ticket — but it produces a visible duplicate.
- **`ticket_message.rfc_message_id-index`** — the same failure mode as above, for `In-Reply-To`/
  `References` threading of a fast follow-up reply.

Primary-table reads (`GetItem`, `BatchGetItem`) use strongly consistent reads by default, so
entity fetches by id are reliable; only the GSI-mediated resolution steps above carry this risk.
