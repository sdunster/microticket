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

## Scope note

This repo is being built in the numbered steps described in the project's build plan (scaffold →
API foundations → auth → instances/memberships → tickets → outbound mail → inbound mail → web →
infra/deploy). Don't reach ahead into a later step's business logic while working on an earlier
one — check what's already in the tree before assuming something doesn't exist yet.
