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

- **Rows reference an instance by `id`, never by `slug`.** Slugs are editable and exist for
  humans; `id` is the foreign key every other table stores. Anything taking an instance from user
  input (the CLI's `--instance`, an argument, a path segment) must resolve it through
  `db::resolve_instance_id`, which accepts either and always returns the id. Skipping that step
  does not fail anywhere: the row is written, listings that look up by the same wrong string find
  it again, and mail still routes — it only surfaces later as an owner whose membership exists but
  who sees no instances, because a resolver looked the instance up by id and found nothing. There
  is a regression test in `tests/inbound_routing_dynamodb_local.rs`.

- **Two senders, deliberately.** System mail (login codes — anything not scoped to an instance)
  sends from `MAIL_FROM`. Instance mail (replies, notifications) sends from that instance's own
  primary inbound address, which is what makes a reply thread back to the right tenant. The
  `FROM_FALLBACK` constant is on a reserved `.test` domain on purpose: a deployment that forgets to
  set `MAIL_FROM` should be refused by the provider rather than quietly send from a domain someone
  else owns.

- **The reply tag carries both ids: `+t{ticket_id}.{reply_token}`.** Not the token alone. The
  token has no index, so resolving by it would need a GSI — and a GSI is eventually consistent, so
  an autoresponder arriving a second after a ticket is created would miss it and open a duplicate.
  Ticket id first makes it a strongly consistent `GetItem`; the token is then compared in constant
  time, and is what stops someone emailing into an arbitrary ticket by guessing a short id.
  `outbound.rs` generates this address and `inbound/routing.rs` parses it — a test asserts the
  round trip, and it is the contract between the two halves of the mail pipeline.

- **Tests that touch environment variables must serialize on a `tokio::sync::Mutex` held across
  every `.await`.** The process environment is global and tests run in parallel, so a test that
  sets a var, releases its lock, and only then awaits leaves a window for another test to change
  it underneath. That failure is invisible locally and shows up in CI. `clippy::await_holding_lock`
  objects to holding a *std* guard across an await — the answer is a tokio mutex, not a shorter
  critical section. See `mail::OVERRIDE_TO_ENV_LOCK` and `turnstile`'s `ENV_LOCK`.

## Deployment notes

- **DNS is not managed by Terraform.** The parent zone lives in a different AWS account, so
  `infra/dns.tf` computes every record that must exist and exposes them as the
  `dns_records_required` output for manual creation. Consequence: the first apply blocks on
  `aws_acm_certificate_validation` waiting for a record it cannot create. That is a documented
  two-phase sequence, not a hang — see `DEVELOPMENT.md` §9.
- **Terraform owns the Lambda functions; CI owns their code.** Each function points at
  `placeholder.zip` with `ignore_changes = [filename, source_code_hash]`, so an apply never fights
  a deploy and the deploy role needs neither `iam:PassRole` nor `lambda:CreateFunction`.
- **The GitHub OIDC subject may be immutable.** GitHub can issue
  `repo:owner@<owner_id>/name@<repo_id>:ref:...` instead of `repo:owner/name:ref:...`, and which
  you get is a repository setting. The trust policy accepts both. If a deploy ever fails with
  `Not authorized to perform sts:AssumeRoleWithWebIdentity` while the policy looks correct, read
  the actual `sub` out of the CloudTrail `AssumeRoleWithWebIdentity` event rather than guessing —
  that is how this was found. Never widen it to a `StringLike` wildcard to make a deploy pass.
- **The Terraform state bucket is created by hand**, not by Terraform — it has to exist before
  `init` can store state in it. `DEVELOPMENT.md` §9.2 has the commands.
- **`make check` validates Terraform under a throwaway `TF_DATA_DIR`.** Once an operator has run a
  real `init` against the S3 backend, a plain `init -backend=false` does not undo it and `validate`
  starts demanding credentials it has no business needing for a static check.

## Scope note

The build plan's numbered steps (scaffold → API foundations → auth → instances/memberships →
tickets → outbound mail → inbound mail → web → infra/deploy) are all complete and deployed.
`SCHEMA.md`'s "Known issues and risks" register is the live list of things known to be imperfect;
open GitHub issues track the rest. Check what is already in the tree before assuming something
does not exist.
