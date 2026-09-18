//! Inbound mail: address routing, ticket resolution, attachment handling,
//! and the orchestration pipeline the SQS-consuming Lambda (and the
//! AWS-free `make local-mail` CLI path) both run.
//!
//! - [`routing`] — recipient → instance resolution (step 4).
//! - [`resolution`] — pure ticket-resolution candidate-building (step 7).
//! - [`attachments`] — filename sanitisation and S3 key layout (step 7).
//! - [`pipeline`] — the impure orchestration: idempotency, parse, loop
//!   guards, resolve, store, notify (step 7). See that module's doc comment
//!   for the full walkthrough.

pub mod attachments;
pub mod pipeline;
pub mod resolution;
pub mod routing;
