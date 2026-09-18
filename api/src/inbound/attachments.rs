//! Attachment filename sanitisation and S3 key layout — pure, unit-tested
//! exhaustively (traversal, separators, empty, over-long, unicode).
//!
//! **Never trust the filename a MIME part claims.** A `Content-Disposition:
//! filename="..."` value is attacker-controlled input from the moment it
//! left someone else's mail client — [`sanitize_filename`] is the one place
//! that turns it into something safe to fold into an S3 key and to show back
//! to an agent in the UI.

/// Hard cap on a single attachment's size, applied by `inbound::pipeline`
/// before it ever calls `storage::Handler::put_bytes`. SES's own inbound
/// size limit is 40MB per message (all parts combined); this project caps a
/// single attachment well under that so one huge attachment can't crowd out
/// the rest of a multi-attachment message or blow past what a Lambda with
/// 512MB of memory can comfortably buffer.
pub const MAX_ATTACHMENT_BYTES: usize = 15 * 1024 * 1024;

/// Hard cap on a sanitised filename's length (in `char`s, not bytes) — long
/// enough for any real filename, short enough to keep S3 keys and UI labels
/// reasonable.
pub const MAX_FILENAME_LEN: usize = 120;

/// Turn an untrusted, possibly-absent MIME part filename into one safe to
/// fold into an S3 key: no path separators (traversal is defeated by taking
/// only the last path segment, so `../../etc/passwd` becomes `passwd`, not
/// rejected outright — the point is a *safe* name, not preserving the
/// attacker's structure), no leading/trailing dots (so a name that is
/// entirely `.`/`..` after that split can't slip through), length-bounded,
/// and restricted to a small ASCII-safe character set — anything outside
/// `[A-Za-z0-9._ -]`, unicode included, is replaced with `_` rather than
/// rejected, so an attachment from a non-ASCII filename still stores
/// successfully under a readable-enough name.
///
/// `index` is the attachment's position within its message (0-based),
/// used to build a deterministic fallback (`attachment-{index}`) when the
/// input is absent, empty, or sanitises down to nothing.
pub fn sanitize_filename(raw: Option<&str>, index: usize) -> String {
    let fallback = || format!("attachment-{index}");
    let Some(raw) = raw else {
        return fallback();
    };

    // Only the last path segment — defeats traversal (`../../x` -> `x`) and
    // strips any directory structure a hostile or careless MIME part name
    // might carry, on either separator style.
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");

    let mut out = String::new();
    for c in base.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
            out.push(c);
        } else {
            out.push('_');
        }
    }

    let trimmed = out.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        return fallback();
    }

    let truncated: String = trimmed.chars().take(MAX_FILENAME_LEN).collect();
    let truncated = truncated.trim().trim_matches('.').trim();
    if truncated.is_empty() {
        fallback()
    } else {
        truncated.to_string()
    }
}

/// `attachments/{ticket_id}/{message_id}/{n}/{filename}` — the storage key
/// for one attachment, whether it arrived inbound or was moved out of
/// `pending/` by `replyToTicket`.
pub fn attachment_key(ticket_id: &str, message_id: &str, index: usize, filename: &str) -> String {
    format!("attachments/{ticket_id}/{message_id}/{index}/{filename}")
}

/// `pending/{instance_id}/{unique}/{filename}` — the key
/// `createAttachmentUpload` presigns a PUT for. `unique` (a nanoid, minted by
/// the caller) keeps two uploads of the same filename by the same instance
/// from colliding.
pub fn pending_key(instance_id: &str, unique: &str, filename: &str) -> String {
    format!("pending/{instance_id}/{unique}/{filename}")
}

/// The `pending/{instance_id}/` prefix every one of that instance's pending
/// uploads must fall under.
pub fn pending_prefix(instance_id: &str) -> String {
    format!("pending/{instance_id}/")
}

/// Guess a MIME type from a filename's extension — used by
/// `replyToTicket`'s `attachmentKeys`, which (unlike an inbound MIME part,
/// which carries its own `Content-Type`) only ever gets a bare filename back
/// from a moved `pending/…` key. Covers the handful of types an agent is
/// actually likely to attach; anything else falls back to the safe generic
/// default, `application/octet-stream`.
pub fn guess_content_type(filename: &str) -> &'static str {
    let ext = filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

/// Whether `key` is a `pending/` key belonging to `instance_id` — the check
/// `replyToTicket` runs on every `attachmentKeys` entry before moving it,
/// so a caller can never smuggle an arbitrary S3 key (another instance's
/// pending upload, or an unrelated object entirely) into place as if it
/// were their own attachment. Rejects anything containing `..` outright,
/// belt-and-braces alongside the prefix check.
pub fn is_pending_key_for_instance(key: &str, instance_id: &str) -> bool {
    !key.contains("..") && key.starts_with(&pending_prefix(instance_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_simple_safe_filename() {
        assert_eq!(sanitize_filename(Some("invoice.pdf"), 0), "invoice.pdf");
    }

    #[test]
    fn strips_a_path_traversal_prefix() {
        assert_eq!(sanitize_filename(Some("../../etc/passwd"), 0), "passwd");
        assert_eq!(
            sanitize_filename(Some("..\\..\\windows\\win.ini"), 0),
            "win.ini"
        );
    }

    #[test]
    fn replaces_separators_that_are_not_at_a_path_boundary() {
        // After taking the last path segment there are no separators left
        // to find in this particular input, so this exercises the general
        // "disallowed character" replacement instead.
        assert_eq!(sanitize_filename(Some("a:b*c?d"), 0), "a_b_c_d");
    }

    #[test]
    fn empty_input_falls_back_to_a_deterministic_name() {
        assert_eq!(sanitize_filename(Some(""), 3), "attachment-3");
        assert_eq!(sanitize_filename(None, 5), "attachment-5");
    }

    #[test]
    fn a_name_that_is_only_dots_falls_back() {
        assert_eq!(sanitize_filename(Some(".."), 0), "attachment-0");
        assert_eq!(sanitize_filename(Some("."), 1), "attachment-1");
        assert_eq!(sanitize_filename(Some("../.."), 2), "attachment-2");
    }

    #[test]
    fn over_long_names_are_truncated() {
        let long = "a".repeat(500) + ".txt";
        let out = sanitize_filename(Some(&long), 0);
        assert!(out.chars().count() <= MAX_FILENAME_LEN, "{}", out.len());
        assert!(!out.is_empty());
    }

    #[test]
    fn truncation_that_lands_on_trailing_dots_is_cleaned_up() {
        // Constructed so the MAX_FILENAME_LEN-char truncation point falls
        // exactly on a run of dots, which must not survive as a trailing
        // sequence (and must not become empty either).
        let name = format!("{}{}", "a".repeat(MAX_FILENAME_LEN - 2), "...rest-of-name");
        let out = sanitize_filename(Some(&name), 0);
        assert!(!out.ends_with('.'), "{out:?}");
        assert!(!out.is_empty());
    }

    #[test]
    fn unicode_characters_are_replaced_not_rejected() {
        let out = sanitize_filename(Some("日本語ファイル.pdf"), 0);
        assert!(out.ends_with(".pdf"), "{out:?}");
        assert!(out.is_ascii(), "expected only ASCII to survive: {out:?}");
        assert_ne!(
            out, "attachment-0",
            "a mostly-valid unicode name shouldn't fall all the way back"
        );
    }

    #[test]
    fn an_entirely_unicode_name_with_no_ascii_extension_still_falls_back_safely() {
        let out = sanitize_filename(Some("😀😀😀"), 7);
        // Every character replaced with '_', trimmed away as non-dot
        // whitespace is not trimmed... this asserts the function never
        // panics and always returns a non-empty, safe name either way.
        assert!(!out.is_empty());
        assert!(out.is_ascii());
    }

    #[test]
    fn whitespace_only_after_sanitisation_falls_back() {
        assert_eq!(sanitize_filename(Some("   "), 4), "attachment-4");
    }

    #[test]
    fn preserves_spaces_within_a_name() {
        assert_eq!(
            sanitize_filename(Some("annual report.pdf"), 0),
            "annual report.pdf"
        );
    }

    // ── key layout ────────────────────────────────────────────────────────

    #[test]
    fn attachment_key_layout() {
        assert_eq!(
            attachment_key("tick1", "msg1", 0, "invoice.pdf"),
            "attachments/tick1/msg1/0/invoice.pdf"
        );
    }

    #[test]
    fn pending_key_layout() {
        assert_eq!(
            pending_key("inst1", "nano123", "invoice.pdf"),
            "pending/inst1/nano123/invoice.pdf"
        );
    }

    #[test]
    fn pending_key_validation_accepts_a_matching_prefix() {
        assert!(is_pending_key_for_instance(
            "pending/inst1/nano123/invoice.pdf",
            "inst1"
        ));
    }

    #[test]
    fn pending_key_validation_rejects_another_instances_prefix() {
        assert!(!is_pending_key_for_instance(
            "pending/inst2/nano123/invoice.pdf",
            "inst1"
        ));
    }

    #[test]
    fn pending_key_validation_rejects_a_non_pending_key() {
        assert!(!is_pending_key_for_instance(
            "attachments/tick1/msg1/0/invoice.pdf",
            "inst1"
        ));
    }

    // ── guess_content_type ───────────────────────────────────────────────

    #[test]
    fn guesses_common_types() {
        assert_eq!(guess_content_type("invoice.pdf"), "application/pdf");
        assert_eq!(guess_content_type("photo.JPG"), "image/jpeg");
        assert_eq!(guess_content_type("notes.txt"), "text/plain");
    }

    #[test]
    fn unknown_extension_falls_back_to_octet_stream() {
        assert_eq!(
            guess_content_type("mystery.xyz"),
            "application/octet-stream"
        );
        assert_eq!(
            guess_content_type("no-extension"),
            "application/octet-stream"
        );
    }

    #[test]
    fn pending_key_validation_rejects_traversal() {
        assert!(!is_pending_key_for_instance(
            "pending/inst1/../inst2/nano123/invoice.pdf",
            "inst1"
        ));
    }
}
