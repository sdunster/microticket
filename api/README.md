# GraphQL API server for Toolbox

> **First-time setup is in [../DEVELOPMENT.md](../DEVELOPMENT.md)** — toolchain, environment,
> and running the full stack with `make dev` / `make dev-local`. This file will grow
> API-specific details (dev server flags, CLI usage, local mail fixtures) as those land.

Prerequisites: Rust via [rustup](https://rustup.rs) — the exact version is pinned in
`rust-toolchain.toml`.

```
cargo test
cargo run --locked --bin export-schema > schema.graphql
```

## OAuth authorization server (for the MCP interface)

`api/src/oauth_http.rs` serves an OAuth 2.1 authorization server so an AI client can act as a
signed-in member, with exactly that member's permissions. All routes sit outside GraphQL and are
served by both `poem` and the Lambda handler:

| Route | What |
|---|---|
| `GET /.well-known/oauth-authorization-server` | RFC 8414 metadata |
| `POST /oauth/register` | RFC 7591 dynamic client registration — public clients only, nothing stored (the `client_id` is the registration JSON + an HMAC) |
| `POST /oauth/token` | `authorization_code` (PKCE S256, single-use code) and `refresh_token` (rotation, with reuse detection) grants |

The consent step is the web page at `/app/oauth/authorize`, backed by the GraphQL query
`oauthAuthorizationRequest` and mutation `approveOauthAuthorization` (both need a signed-in user
session). Tokens are `mtoa_…`/`mtor_…` and are **not** accepted by `/graphql`.

Configuration: `OAUTH_CLIENT_ID_SECRET` (signs client ids; registration answers `503` without it),
`API_BASE_URL` (the OAuth issuer — **required behind CloudFront**, which doesn't forward `Host`)
and `APP_BASE_URL` (where the consent page lives). `local/local.env` sets all three for local dev.

### Connected AI apps (list + revoke)

Once a member has approved a client, `me { oauthGrants }` lists their authorized grants: client
name, redirect host, scope, created/last-used timestamps, and `refreshExpiresAt` (when the grant
goes dead if never used again). Token hashes and the client id are never exposed. Expired grants
are filtered out at read time, since DynamoDB's TTL deletion lags real expiry.

`revokeOauthGrant(id)` deletes a grant outright, so its access token stops verifying immediately.
Both are **self-only, superusers included** (the same posture as `User.passkeys`): anything else
fails the same way a missing grant would, so a caller can't probe for other users' grant ids. There
is no "revoke on disable": disabling a user already blocks every credential kind via
`fetch_update_user_auth_info`, and re-enabling restores access the way it does for other tokens.

The web page is the "Connected AI apps" section of `/app/settings`.
