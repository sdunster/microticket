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
    `submitVerifiedTicket` sends the same acknowledgement to every `to`/`cc` address — see the "API
    tokens" entry below.
  - `setTicketStatus` sends requesters/CCs a brief notice (`kind: SYSTEM`) **only on a close or a
    reopen** (including restoring a deleted ticket back to open) — see `outbound::status_notice_body`.
    Transitioning *into* `DELETED`, in either direction, sends nothing: deleting a ticket is admin
    housekeeping (spam cleanup, a mistaken submission), not a resolution the customer is owed a
    notification about.
  - `assignTicket`, `addTicketRequester`/`removeTicketRequester`, `addTicketCc`/`removeTicketCc`
    send no **customer** mail at all — which agent owns a ticket, and who else is copied on it, is
    internal bookkeeping. `addInternalNote` never sends customer mail, as the build plan says
    explicitly. (`assignTicket` *does* send a best-effort **staff** notice — see the next entry;
    the requester/CC mutations never send anything, staff included.)
  - A send failure is handled differently depending on whether the mail *is* the mutation's
    primary effect or a secondary side effect of one: `replyToTicket` fails the mutation (the
    message row survives, but the caller is told delivery didn't happen); `submitTicket`'s
    acknowledgement and `setTicketStatus`'s notice are best-effort — logged on failure, never
    surfacing as a mutation error, since the ticket already exists / the status already changed by
    the time mail is attempted. See `reply_to_ticket`'s doc comment in `graphql/mutations.rs` for
    the full reasoning.

- **Staff notifications: a second, separate mail pipeline (`api/src/staff_notify.rs`).** The
  customer-mail rule above is about requesters/CCs. Members/agents get their own opt-out email
  notifications about ticket activity in their instance, stored as five optional `Bool` attributes
  on the `membership` row (absent means the default below, per the omit-optional-attributes house
  rule; `updateNotificationSettings` writes an explicit `true`/`false` once changed; removing and
  re-adding a membership resets to defaults):

  | Setting | Attribute | Default | Meaning |
  |---|---|---|---|
  | `newTicket` | `notify_new_ticket` | `true` | A new ticket is opened (inbound email or the public submit form) |
  | `assignedToMe` | `notify_assigned_to_me` | `true` | Someone else assigns a ticket to this member |
  | `assignedToMeUpdated` | `notify_assigned_to_me_updated` | `true` | A ticket assigned to this member gets an update (also covers being unassigned/reassigned away — see below) |
  | `unassignedUpdated` | `notify_unassigned_updated` | `true` | An unassigned ticket gets an update |
  | `assignedToOthersUpdated` | `notify_assigned_to_others_updated` | `false` | A ticket assigned to someone else gets an update — off by default; opting into every other agent's traffic is a deliberate choice |

  "An update" is a customer message (inbound mail on an existing ticket), an agent reply
  (`replyToTicket`, only after the customer-facing send succeeds), an internal note
  (`addInternalNote`), or a status change (`setTicketStatus`) — **except a transition *into*
  `DELETED`**, mirroring the customer-mail carve-out above. Assignment (`assignTicket`) is its own
  event: it mails only the new assignee (`assignedToMe`) and the previous assignee
  (`assignedToMeUpdated`, told they were unassigned or the ticket went to someone else) — it never
  fans out to the rest of the team, and a self-assign or a no-op reassignment notifies nobody. The
  actor who performed the action is always excluded, superusers get nothing by virtue of being
  superusers (only a real membership row counts), and a deleted ticket notifies nobody regardless
  of event or settings. See `staff_notify::select_recipients`'s doc comment for the exact rules,
  unit-tested there.

  A staff notice is deliberately built differently from customer mail (`staff_notify::build_staff_mime`,
  not `outbound::build_outbound`): `From` the system sender (`mail::system_from()`) under the
  instance's display name, `Reply-To` the system reply-to, **no `+t{ticket_id}.{reply_token}` tag
  anywhere**, one message per recipient (`To:` only, no `Cc:`), a subject that does not parse as
  `[#{slug}-{number}]` (`outbound::parse_subject_tag` must return `None` for it — pinned by a unit
  test), and never persisted as a `ticket_message` row. The reason is structural, not cosmetic: SES
  receives every address on the support domain, so a staff member hitting "reply" on a notice that
  *did* carry a reply tag would have their reply mistaken for a customer message by the inbound
  pipeline. Like `submitTicket`'s acknowledgement and `setTicketStatus`'s notice, sending is
  best-effort — every failure is `warn!`-logged and swallowed, never surfacing as a mutation error
  or failing inbound processing.

  `updateNotificationSettings` (GraphQL) is the only way to change these, and only for the caller's
  *own* membership — resolved via `list_memberships_by_user(caller)` filtered to the given
  instance, never a trusted membership id. `MembershipInfo.notificationSettings` is **self-only**:
  `FORBIDDEN` for anyone reading someone else's `memberships` (e.g. a superuser via `adminUser`),
  the same defence-in-depth posture as `User.passkeys`.

  Needs `APP_BASE_URL` (the web app's origin, for the "View ticket"/"Change your notification
  settings" links a notice carries) set in every environment that sends this mail — `api/src/staff_notify.rs`
  defaults to `http://localhost:5173` otherwise. The inbound-mail Lambda also needs `MAIL_FROM` now
  that it sends mail of its own (previously it only consumed the inbound queue) — without it, a
  staff notification sent from that Lambda falls back to `mail::FROM_FALLBACK`'s reserved `.test`
  domain and is refused by the provider.

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

- **Superuser boundary: admin + instance settings, never ticket access; grantable only via the
  CLI.** `db::User::superuser` gates the `Superuser`/`InstanceOwnerOrSuperuser` GraphQL guards —
  instance/user/membership/inbound-address management (`createInstance`, `createUser`,
  `addMember`, `addInboundAddress` via `InstanceOwnerOrSuperuser`, and friends) — and nothing else.
  A superuser does **not** pass `Member`/`InstanceOwner`, has no implicit access to any instance's
  tickets, and does not appear in `User.memberships`/the instance switcher unless they hold a real
  membership row. No GraphQL mutation can set `superuser`; the only way to grant or revoke it is
  `bin/cli.rs`'s `user set-superuser`. Don't widen this — a future "superuser can see all tickets"
  feature needs its own explicit guard, not a loosening of `Superuser`/`InstanceOwnerOrSuperuser`.

- **API tokens (`mta_`) authorise `submitVerifiedTicket` and nothing else.** `AuthInfo::ApiToken`
  is a third principal, instance-scoped, minted only by that instance's owner or a superuser
  (`createApiToken`, guarded `InstanceOwnerOrSuperuser` like `addInboundAddress` — an integration
  credential is instance settings, not something a plain agent hands out). It never passes
  `Authenticated` or any other guard — including `Superuser`/`InstanceOwnerOrSuperuser` themselves —
  so a leaked token reaches exactly one mutation, never `createAttachmentUpload`, never `me`, never
  the token-management mutations that could mint or revoke more of itself.
  - **Id embedded in the token, no `token_hash` GSI** — same shape, same reasoning, as the
    `+t{ticket_id}.{reply_token}` reply tag two entries below: `mta_{id}.{secret}`, verified by a
    `GetItem` on `id` (no GSI, no eventual-consistency window) followed by a constant-time compare
    of the full token against the stored `token_hash`. A token must authenticate on its very first
    use, which a GSI lookup cannot promise.
  - `submitVerifiedTicket(subject, body, to, cc)` takes the instance from the token, never an
    argument, and takes `to`/`cc` **on the caller's word** — no email-verification code, unlike the
    public submit form's `Requester` token. That trust is the entire point of an owner/superuser-
    minted credential: an external service that has already verified its own users' addresses (a
    customer portal behind its own login) opens a ticket with the right requester/CC list in one
    call. `to`/`cc` are normalized, deduped, and capped at 20 combined recipients — a leaked token
    must not become a bulk-mail relay — and any address matching one of the instance's own inbound
    addresses is rejected outright, not silently dropped (dropping it could leave a ticket whose
    requester is itself).
  - No expiry: a long-lived integration credential, not a session. Revocation is
    `updateApiToken(enabled: false)` or `deleteApiToken`.

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
