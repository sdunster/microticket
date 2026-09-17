# GraphQL API server for microticket

> **First-time setup is in [../DEVELOPMENT.md](../DEVELOPMENT.md)** — toolchain, environment,
> and running the full stack with `make dev` / `make dev-local`. This file will grow
> API-specific details (dev server flags, CLI usage, local mail fixtures) as those land.

Prerequisites: Rust via [rustup](https://rustup.rs) — the exact version is pinned in
`rust-toolchain.toml`.

```
cargo test
cargo run --locked --bin export-schema > schema.graphql
```
