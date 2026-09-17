# Database Schema Reference

> **Status:** this file is a skeleton written during the scaffold step. No tables exist yet — the
> data model lands in later build steps. This documents the intended shape so it can be filled in
> table-by-table as each one is created, rather than written speculatively ahead of the code.

DynamoDB, multi-table, one table per entity, named `{DB_PREFIX}_{entity}`. Hash key `id` is a
12-char nanoid unless noted otherwise. GSIs are named `{hashAttr}-{rangeAttr}-index`.

**House rule:** optional attributes are omitted, never written as `Null` — see `CLAUDE.md`.
Deletion of an optional attribute removes it (dropping the row out of any sparse GSI that
projects it) rather than nulling it.

## Tables

_(Fill in one `### \`{prefix}_{entity}\`` section per table as it's implemented, in the shape:
hash key, GSIs, attributes with types/meaning, and any write-time invariants. Planned tables,
per the build plan:)_

- `instance`
- `inbound_address`
- `user`
- `membership`
- `ticket`
- `ticket_message`
- `counter`
- `login_code`
- `user_token`
- `webauthn_credential`
- `ephemeral_state`
- `processed_message`

## Known issues and risks

_(later steps — race conditions, correctness caveats, GSI eventual-consistency notes, and
anything else worth flagging, once there's code to say it about.)_
