# local/

Local-development-only tooling: not part of the deployed app, not built by CI beyond the checks
below. Everything here is AWS-free and needs no credentials — see [../DEVELOPMENT.md](../DEVELOPMENT.md)
and the `local-*` targets in the root `Makefile`.

## What's here

- **`dynamodb.sh`** (`start|stop|reset|fetch|status`) — runs a local DynamoDB, separate from any
  real AWS account. Prefers a plain Java process (`fetch` downloads and checksum-verifies
  Amazon's DynamoDB Local tarball into `dynamodb-local/`, gitignored; needs only a JRE 17+), and
  falls back to the `amazon/dynamodb-local` container via `docker-compose.yml` when
  `LOCAL_DDB=docker` is set or no Java distribution is available. Data persists under the
  gitignored `.dynamodb-data/` between runs; `reset` deletes it.
- **`docker-compose.yml`** — the Docker fallback path, on port **8100** (8000 is the API). Also
  runs `dynamodb-admin`, a browser UI for poking at local tables, on :8101.
- **`local.env`** — committed, non-secret environment for the local stack: `DB_PREFIX=local`,
  dummy AWS credentials, `AWS_ENDPOINT_URL_DYNAMODB=http://localhost:8100`, WebAuthn RP config,
  `TURNSTILE_DISABLED=1`. `make dev-local` exports it before starting anything — exported
  variables beat the repo-root `.env`, since dotenvy never overrides a variable that's already
  set, so this file decides where a local run points.
- **`e2e.sh`** (`up|down|status`) — the detached counterpart to `make dev-local`, for scripts and
  CI rather than a terminal: starts DynamoDB Local, creates tables, builds and starts the API,
  compiles Relay and starts the web dev server, then returns once both answer. `down` kills the
  API/web process group (DynamoDB Local keeps running — stop it separately with `make
  local-down`).

## What's still coming (later build steps)

- **Seed fixtures** — JSON fixtures loaded by `api`'s `local-seed` binary: two instances, an
  owner and an agent, inbound addresses, and a couple of tickets. `make local-seed` is a stub
  until the entity types it needs exist.
- **Mail fixtures** — a `mail/` directory of raw `.eml` files exercised by `make local-mail`,
  covering a new ticket, a `+tag` reply, an `In-Reply-To` reply, a reply from an unknown sender,
  an autoresponder (which must be dropped), an attachment, and a wildcard-address match. Mocked
  outbound mail lands in the gitignored `mail-out/` (see `MOCK_MAIL_DIR` in `local.env`).

## Table definitions

`api/src/bin/local-tables.rs` creates the tables this local database needs. It is transcribed **by
hand** from `infra/dynamodb.tf` — that file is the schema's source of truth, since DynamoDB Local
has no Terraform provider to drive it directly. `make local-tables-check` (part of
`.github/workflows/_check-local.yml`) fails if the two drift apart.
