//! Inbound-mail address resolution. The SQS-consuming Lambda itself (parsing,
//! threading, attachments, loop guards) is step 7's job — see `CLAUDE.md`'s
//! scope note. This step only needs enough of "which instance does this
//! recipient belong to" to support the admin CLI and seed fixtures, but it's
//! written to be exactly what step 7 will call.

pub mod routing;
