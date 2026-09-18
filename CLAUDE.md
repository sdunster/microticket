# CLAUDE.md

Guidance for working on microticket. See `README.md` for the product pitch, `DEVELOPMENT.md` for
local setup, and `SCHEMA.md` for the data model.

## House rules (apply from the first commit)

- **DynamoDB: omit optional attributes, never write `Null`.** An absent attribute means "not
  set"; writing an explicit `Null` breaks sparse GSIs (an attribute has to be *absent*, not
  null, for a row to drop out of a GSI that projects it) and complicates hydration. Deleting or
  clearing an optional value means removing the attribute (`REMOVE` in an update expression), not
  setting it to null.
- **One commit per PR, and every commit must be independently deployable.** Squash before
  opening/merging a PR. Don't leave a PR in a half-working state that only becomes correct once a
  later PR lands — see `CONTRIBUTING.md`.
- **`make check` is static-only — it runs no tests.** It covers actionlint, Relay compilation,
  Prettier, ESLint, `tsc`, a production web build, `terraform fmt`/`validate`, `cargo fmt`, the
  GraphQL schema diff, and Clippy. `make test` is the separate target that actually runs the Rust
  and web test suites. Both must pass before a PR is opened.
- **Regenerate `schema.graphql` after any GraphQL change:**
  ```bash
  cd api && cargo run --locked --bin export-schema > schema.graphql
  cd web && npm run relay
  ```
  CI diffs the committed `api/schema.graphql` against a fresh export and fails if they differ.
- **Mutations require `--enable-mutations` on the dev server.** `cargo run --bin poem` (or
  `poem-local`) starts read-only by default; pass `--enable-mutations` to allow writes. This is a
  deliberate guard against accidentally mutating whatever `DB_PREFIX` you're pointed at.
- **No queue abstraction in `api/src/app.rs`.** seslogin has `HasQueues`/`queue.rs`/`sqs.rs`/
  `mockqueue.rs` because its API *produces* to SQS (member sync, NITC export, healthchecks).
  microticket's API never produces to SQS — the only queue in this system carries inbound mail,
  and that queue is *consumed* by the inbound-mail Lambda (step 7), a separate binary with no
  GraphQL surface. Don't add a queue trait to `app.rs`/`MyApp` unless the API itself starts
  producing to a queue.
- **Outbound mail: which ticket updates email.** The build plan says "every ticket update that is
  not an internal note mails requesters + CCs" — read literally that would include assignment and
  requester/CC-list edits, which would mean a customer gets an email every time a ticket changes
  hands internally. That's noise, not signal, so the actual rule implemented (`graphql/mutations.rs`)
  is narrower:
  - `replyToTicket` sends the reply itself (`kind: REPLY`) — the agent's own words, always.
  - `submitTicket` sends the requester a brief acknowledgement (`kind: SYSTEM`), so the public
    form's result lands in their inbox with the `Reply-To` thread already wired up.
  - `setTicketStatus` sends requesters/CCs a brief notice (`kind: SYSTEM`) **only on a close or a
    reopen** (including restoring a deleted ticket back to open) — see `outbound::status_notice_body`.
    Transitioning *into* `DELETED`, in either direction, sends nothing: deleting a ticket is admin
    housekeeping (spam cleanup, a mistaken submission), not a resolution the customer is owed a
    notification about.
  - `assignTicket`, `addTicketRequester`/`removeTicketRequester`, `addTicketCc`/`removeTicketCc`
    send nothing at all — which agent owns a ticket, and who else is copied on it, is internal
    bookkeeping. `addInternalNote` never sends mail, as the build plan says explicitly.
  - A send failure is handled differently depending on whether the mail *is* the mutation's
    primary effect or a secondary side effect of one: `replyToTicket` fails the mutation (the
    message row survives, but the caller is told delivery didn't happen); `submitTicket`'s
    acknowledgement and `setTicketStatus`'s notice are best-effort — logged on failure, never
    surfacing as a mutation error, since the ticket already exists / the status already changed by
    the time mail is attempted. See `reply_to_ticket`'s doc comment in `graphql/mutations.rs` for
    the full reasoning.

## Scope note

This repo is being built in the numbered steps described in the project's build plan (scaffold →
API foundations → auth → instances/memberships → tickets → outbound mail → inbound mail → web →
infra/deploy). Don't reach ahead into a later step's business logic while working on an earlier
one — check what's already in the tree before assuming something doesn't exist yet.
