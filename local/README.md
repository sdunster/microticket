# local/

Local-development-only tooling: not part of the deployed app, not built by CI beyond the checks
below.

> **Status:** this directory is a placeholder from the scaffold step of the build. The pieces it
> will hold land in later steps — nothing here yet actually runs.

What's coming here, per the build plan:

- **DynamoDB Local** — a `dynamodb.sh` runner (Java or Docker, whichever the machine has) plus
  `local.env` config, driving a local database separate from anything in AWS. Data lives under
  the gitignored `.dynamodb-data/`.
- **Seed fixtures** — JSON fixtures loaded by `api`'s `local-seed` binary: two instances, an
  owner and an agent, inbound addresses, and a couple of tickets.
- **Mail fixtures** — a `mail/` directory of raw `.eml` files exercised by `make local-mail`,
  covering a new ticket, a `+tag` reply, an `In-Reply-To` reply, a reply from an unknown sender,
  an autoresponder (which must be dropped), an attachment, and a wildcard-address match. Mocked
  outbound mail lands in the gitignored `mail-out/`.
- **A detached e2e stack** (`e2e.sh`) for driving the app from a script without holding a
  terminal.

None of this touches AWS or needs credentials — see [../DEVELOPMENT.md](../DEVELOPMENT.md) and
the `local-*` targets in the root `Makefile` (currently stubs).
