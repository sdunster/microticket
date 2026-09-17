//! Build facts about the running server.
//!
//! Baked in by `build.rs` so the deployed Lambda binary carries its own commit
//! SHA without needing a `terraform apply` per deploy (the deploy workflow runs
//! `cargo lambda deploy` with no `--env-var`, so the Lambda's environment map is
//! owned entirely by Terraform). Mirrors `VITE_CLIENT_VERSION` on the web side.

/// Git commit this binary was built from, or `"dev"` when the build had no git
/// context.
pub const GIT_REV: &str = env!("MICROTICKET_GIT_REV");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_rev_is_populated() {
        assert!(!GIT_REV.is_empty());
    }
}
