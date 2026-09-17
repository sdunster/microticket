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

_(later step)_ `make dev-local` brings up DynamoDB Local plus mocked SES/SQS so the whole stack —
including inbound mail — runs with no AWS account. This section will cover: one-time setup of
DynamoDB Local, seeding fixture data (`make local-seed`), feeding a raw `.eml` fixture through the
inbound pipeline (`make local-mail FILE=...`), and reading mocked outbound mail back out of
`local/mail-out/`.

## 6. Bypassing auth for local UI work

_(later step, once auth exists)_ Document any `--dev-auth-*` flag the dev server grows, mirroring
seslogin's, if one is added.

## 7. Troubleshooting

_(later step — fill in as real issues come up; don't invent hypothetical ones.)_

## Where to go next

- [SCHEMA.md](SCHEMA.md) — the data model.
- [README.md](README.md) — product overview, project layout, branches.
- [CONTRIBUTING.md](CONTRIBUTING.md) — PR process and checks.
