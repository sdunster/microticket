//! Real S3 implementation of [`crate::storage::Handler`].
//!
//! Bucket name comes from the `MAIL_BUCKET` environment variable — never
//! hardcoded, so the same binary works against whatever bucket Terraform (a
//! later step) provisions, and so nothing here can leak a real bucket name
//! into this public repo.

use anyhow::{Context, Result, anyhow};
use aws_sdk_s3::Client;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;

use crate::storage::{self, PRESIGN_EXPIRY};

pub struct Storage {
    client: Client,
    bucket: String,
}

impl Storage {
    /// Reads `MAIL_BUCKET` at construction — a missing value is a
    /// configuration bug, so this fails loudly rather than deferring the
    /// error to the first call that needs it.
    pub async fn new() -> Result<Self> {
        let bucket = std::env::var("MAIL_BUCKET")
            .context("MAIL_BUCKET must be set for the S3-backed storage handler")?;
        let config = crate::aws_config_loader().load().await;
        Ok(Self {
            client: Client::new(&config),
            bucket,
        })
    }
}

impl storage::Handler for Storage {
    async fn put_bytes(&self, key: &str, bytes: &[u8], content_type: &str) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(bytes.to_vec()))
            .send()
            .await
            .with_context(|| format!("putting s3://{}/{key}", self.bucket))?;
        Ok(())
    }

    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .with_context(|| format!("getting s3://{}/{key}", self.bucket))?;
        let bytes = resp
            .body
            .collect()
            .await
            .with_context(|| format!("reading s3://{}/{key}", self.bucket))?;
        Ok(bytes.into_bytes().to_vec())
    }

    async fn move_object(&self, from_key: &str, to_key: &str) -> Result<()> {
        // S3 has no rename — copy then delete the source. A crash between
        // the two leaves the object at both keys momentarily (never at
        // neither), which is the safe direction to fail in: at worst a
        // `pending/…` key lingers past its 1-day lifecycle expiry instead of
        // an attachment silently vanishing.
        let source = format!("{}/{from_key}", self.bucket);
        self.client
            .copy_object()
            .bucket(&self.bucket)
            .copy_source(&source)
            .key(to_key)
            .send()
            .await
            .with_context(|| format!("copying s3://{source} to {to_key}"))?;
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(from_key)
            .send()
            .await
            .with_context(|| format!("deleting s3://{}/{from_key} after move", self.bucket))?;
        Ok(())
    }

    async fn presign_put(&self, key: &str, content_type: &str) -> Result<String> {
        let presign_config =
            PresigningConfig::expires_in(PRESIGN_EXPIRY).map_err(|e| anyhow!("{e}"))?;
        let req = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .presigned(presign_config)
            .await
            .with_context(|| format!("presigning PUT for {key}"))?;
        Ok(req.uri().to_string())
    }

    async fn object_size(&self, key: &str) -> Result<u64> {
        let resp = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .with_context(|| format!("heading s3://{}/{key}", self.bucket))?;
        Ok(resp.content_length().unwrap_or(0).max(0) as u64)
    }

    async fn presign_get(&self, key: &str) -> Result<String> {
        let presign_config =
            PresigningConfig::expires_in(PRESIGN_EXPIRY).map_err(|e| anyhow!("{e}"))?;
        let req = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presign_config)
            .await
            .with_context(|| format!("presigning GET for {key}"))?;
        Ok(req.uri().to_string())
    }

    async fn presign_get_download(&self, key: &str, filename: &str) -> Result<String> {
        let presign_config =
            PresigningConfig::expires_in(PRESIGN_EXPIRY).map_err(|e| anyhow!("{e}"))?;
        let safe = storage::sanitize_download_filename(filename);
        let req = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .response_content_disposition(format!("attachment; filename=\"{safe}\""))
            .presigned(presign_config)
            .await
            .with_context(|| format!("presigning download GET for {key}"))?;
        Ok(req.uri().to_string())
    }
}
