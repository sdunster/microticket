//! Object-storage abstraction for the mail bucket: raw MIME, attachments, and
//! the presigned upload/download flow.
//!
//! Mirrors the [`crate::mail`] / [`crate::sesmail`] / [`crate::mockmail`]
//! split: this module holds the trait, [`crate::s3storage`] backs it with
//! real S3 (bucket name from the `MAIL_BUCKET` env var — never hardcoded),
//! and [`crate::mockstorage`] backs it with a local directory so the whole
//! inbound pipeline — and `createAttachmentUpload`/`replyToTicket`'s
//! attachment move — can be exercised with no AWS account (`make
//! local-mail`, `make dev-local`).
//!
//! Every key is bucket-relative. Layout (see `api/src/inbound/attachments.rs`
//! for the functions that build these):
//!
//! - `inbound/{ses_message_id}` — raw MIME, written by SES's own S3 action
//!   (the real Lambda only ever reads this key; nothing here ever writes it).
//! - `attachments/{ticket_id}/{message_id}/{n}/{filename}` — a stored
//!   attachment, whether it arrived inbound or was uploaded by an agent.
//! - `pending/{instance_id}/{nanoid}/{filename}` — an agent's upload
//!   (`createAttachmentUpload`) not yet attached to a message; moved into
//!   its final `attachments/…` key by `replyToTicket`.

use anyhow::Result;
use std::future::Future;
use std::time::Duration;

/// How long a presigned URL stays valid. Generous enough for a slow upload
/// or a browser tab left open, short enough that a leaked URL is not a
/// standing liability.
pub const PRESIGN_EXPIRY: Duration = Duration::from_secs(15 * 60);

/// `Sync` for the same reason as [`crate::db::Handler`]/[`crate::mail::Handler`]:
/// a `&impl Handler` is held across `.await` inside the `Send` futures the
/// GraphQL/Poem stack builds.
pub trait Handler: Sync {
    /// Write bytes directly — used by the inbound pipeline to store parsed
    /// attachments, and (for the AWS-free `local-mail` path, which has no
    /// SES S3 action to have written it already) the raw MIME itself.
    fn put_bytes(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Read bytes directly — the inbound Lambda's fetch of the raw MIME SES
    /// already wrote to `inbound/{ses_message_id}`.
    fn get_bytes(&self, key: &str) -> impl Future<Output = Result<Vec<u8>>> + Send;

    /// Move an object from one key to another (copy + delete for S3; a
    /// rename for the local mock) — how `replyToTicket` moves a
    /// `pending/…` upload into its final `attachments/…` key.
    fn move_object(&self, from_key: &str, to_key: &str) -> impl Future<Output = Result<()>> + Send;

    /// A presigned PUT URL for `createAttachmentUpload`.
    fn presign_put(
        &self,
        key: &str,
        content_type: &str,
    ) -> impl Future<Output = Result<String>> + Send;

    /// A presigned GET URL for an attachment download.
    fn presign_get(&self, key: &str) -> impl Future<Output = Result<String>> + Send;

    /// The size, in bytes, of an already-stored object — used after
    /// `replyToTicket` moves a `pending/…` upload into place, to fill in
    /// `Attachment.size` without re-reading the whole object.
    fn object_size(&self, key: &str) -> impl Future<Output = Result<u64>> + Send;
}
