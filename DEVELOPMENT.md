# Development setup

> **Status:** this file is a skeleton written during the scaffold step of the build. Sections
> marked _(later step)_ describe things that don't exist in the tree yet — fill them in as the
> corresponding step lands, rather than writing ahead of the code.

## 1. Install the toolchain

### Rust

Install via [rustup](https://rustup.rs). The exact compiler version is pinned in
`api/rust-toolchain.toml` — `rustup` picks it up automatically when you run anything under
`api/`. `make check-toolchain` verifies your active `rustc` matches the pin.

### Node

Node.js >= 22 (`web/.npmrc` sets `engine-strict=true`, so an older Node fails installs loudly
rather than producing confusing errors later).

### Dependencies

```bash
cd web && npm i
```

### Optional

- `actionlint` (`brew install actionlint`) — lints `.github/workflows/*.yml`, run by
  `make gha-lint` (part of `make check`).
- Terraform — required for `make check`'s infra formatting/validation step.

## 2. Environment

Copy `web/.env.local.example` to `web/.env.local`. The repo-root `.env` (committed, non-secret)
sets sane local defaults (`DB_PREFIX`, WebAuthn RP id/origin, `TURNSTILE_DISABLED=1`). Secrets —
`JWT_SECRET` and, optionally, `TURNSTILE_SECRET_KEY` — go in `.env.secret`, copied from
`.env.secret.example` and never committed. _(The auth step wires these up; they aren't consumed by
anything yet.)_

## 3. Run it

```bash
make dev-local   # DynamoDB Local + mocked SES/SQS — no AWS account needed
make dev         # against real AWS DynamoDB tables — needs AWS credentials
```

Both build the API, wait for it to answer on `:8000`, then start the Relay compiler in watch
mode and the Vite dev server on `:5173`. See the `run_dev` macro in the `Makefile`.

_(later step: AWS credential setup / SSO profile instructions, once `infra/` is applied and there
is a real account to point at.)_

## 4. Everyday commands

```bash
make check          # static checks only — no tests (see CLAUDE.md)
make test            # cargo test + web unit tests
make format          # cargo fmt + prettier + terraform fmt
make lint            # actionlint + clippy + eslint
```

### After changing the GraphQL API

```bash
cd api && cargo run --locked --bin export-schema > schema.graphql
cd web && npm run relay
```

`make check` diffs the committed `api/schema.graphql` against a fresh export and fails if it's
stale.

## 5. Running without AWS

```bash
make dev-local                  # DynamoDB Local + mocked SES/SQS — no AWS account needed
make local-seed                 # writes local/seed/synthetic.json into the local DB
make local-clear                # deletes app-written rows (tokens, WebAuthn state, ...), keeps the seed
```

`make local-seed` (`api/src/bin/local-seed.rs apply`) writes two seeded instances, an owner and an
agent, a handful of inbound addresses (including a wildcard), and a ready-made session token for
each user — all as raw DynamoDB items from `local/seed/synthetic.json`, so their ids are exactly as
committed. Nothing in the fixture is real: `api/tests/seed_fixtures.rs` pins that every address is
`@example.com`/`@microticket.test` and refuses anything else.

Seeded accounts (only ever valid against a `local`-prefixed database — never real):

| Role  | Email                     | Instance(s)          | Ready-made token                        |
| ----- | ------------------------- | --------------------- | ---------------------------------------- |
| owner | `owner@microticket.test`  | `acme`, `ridgeline`   | `mtu_localdev0000000000000000000owner`  |
| agent | `agent@microticket.test`  | `acme`                | `mtu_localdev0000000000000000000agent`  |

Use a token directly (`Authorization: Bearer mtu_localdev...`) to skip the email-code flow entirely
when poking at the API by hand (`curl`, GraphiQL at `http://localhost:8000/`), or log in normally
with the seeded emails — `poem-local`'s mocked mailer prints the 6-digit code to the API's own log.

`make local-clear` deletes everything the *running app* writes on top of the seed (session tokens
minted by a real login, WebAuthn credentials, submit codes/tokens) without touching the seeded rows
themselves, so a re-`local-seed` afterward is a clean overwrite rather than a pile-up. `make
local-reset` is the blunter tool: it destroys and rebuilds every table, discarding the seed too.

_(later step: feeding a raw `.eml` fixture through the inbound pipeline (`make local-mail
FILE=...`), and reading mocked outbound mail back out of `local/mail-out/`.)_

## 6. The admin CLI and bootstrapping an organisation

`api/src/bin/cli.rs` (`cargo run --bin cli --`) is the operator tool for inspecting and writing the
DB directly — instances, inbound addresses, users, and memberships. It writes immediately; pass the
global `--dry-run` flag to see what a command *would* do without writing anything. It takes
`--db-prefix` (or the `DB_PREFIX` env var) and, unlike `local-tables`/`local-seed`, is **not**
restricted to a local database — this is the same tool that bootstraps the first real organisation
in prod.

```bash
cd api && cargo run --bin cli -- --help
```

Bootstrap sequence for a brand-new deployment's first organisation and owner:

```bash
# 1. Create the first user (the person who will own the organisation).
cargo run --bin cli -- user create owner@yourdomain.com "Your Name"

# 2. Create the instance (the tenant organisation).
cargo run --bin cli -- instance create "Your Company Support" your-company \
    --from-name "Your Company Support" \
    --signature "Thanks, Your Company Support" \
    --public-submission-enabled

# 3. Grant that user the owner role in the instance (member invites are a CLI-only
#    operation for now — there is no invite mutation over GraphQL).
cargo run --bin cli -- member add --instance <instance_id> --user owner@yourdomain.com --role owner

# 4. Map the instance's real inbound address(es).
cargo run --bin cli -- address add --instance <instance_id> support@yourdomain.com
```

Against the local stack, export `local/local.env` first (`set -a && . ../local/local.env && set
+a`, from `api/`) so the CLI points at DynamoDB Local instead of a real account — or just run `make
local-seed`, which does the local-stack equivalent of this whole sequence for you, twice over, from
`local/seed/synthetic.json`.

## 7. Bypassing auth for local UI work

`--dev-auth-user <id-or-email>` on `poem`/`poem-local` bypasses token verification entirely and
treats every request as that user, with their *real* permissions (memberships included) — never a
synthetic elevated principal. Only a `User` principal can be impersonated (microticket has no
kiosk/session-equivalent). Never enable this in a deployed environment; the Lambda binary has no
CLI to read the flag from in the first place, so it is unreachable there by construction.

## 8. Troubleshooting

_(later step — fill in as real issues come up; don't invent hypothetical ones.)_

## Where to go next

- [SCHEMA.md](SCHEMA.md) — the data model.
- [README.md](README.md) — product overview, project layout, branches.
- [CONTRIBUTING.md](CONTRIBUTING.md) — PR process and checks.
