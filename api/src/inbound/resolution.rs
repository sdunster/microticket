//! Ticket resolution for inbound mail: which existing ticket (if any) a
//! parsed message belongs to, in the priority order the build plan
//! specifies.
//!
//! **The design change this step makes** (see `CLAUDE.md` and `SCHEMA.md`
//! for the full reasoning): the build plan originally specified the reply
//! tag as `+t{reply_token}`, matching against `reply_token` — but that field
//! has no GSI, and adding one would make resolution depend on an
//! *eventually consistent* index, so an auto-reply arriving a second after a
//! ticket is created could race the index and wrongly open a duplicate
//! ticket. The tag now carries **both** pieces: `+t{ticket_id}.{reply_token}`.
//! `ticket_id` drives a strongly-consistent `GetItem` on the ticket's hash
//! key (no GSI, no eventual-consistency window); `reply_token` is still
//! compared — with a constant-time comparison, [`crate::inbound::resolution::constant_time_eq`]
//! — because it remains the actual security boundary: without it, anyone
//! who can guess or enumerate a 12-char ticket id could email into someone
//! else's ticket.
//!
//! This module is pure by construction — no I/O — mirroring
//! [`crate::inbound::routing`]. [`build_resolution_plan`] decides *what* to
//! try and in what order; `inbound::pipeline` is the impure caller that
//! walks the plan against the database, verifying at each step that a
//! resolved ticket belongs to the instance the mail actually routed to (see
//! that module's doc comment for the cross-tenant rejection this guards
//! against).

/// A parsed `+t{ticket_id}.{reply_token}` tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyTag {
    pub ticket_id: String,
    pub reply_token: String,
}

/// Parse a recipient's `+tag` local-part suffix (as
/// [`crate::inbound::routing::normalize_recipient`] extracts it — the
/// leading `+` already stripped) into a [`ReplyTag`], or `None` if it isn't
/// shaped like one. The format is `t{ticket_id}.{reply_token}`: a literal
/// `t` marker (kept from the original design, so a tag is visually
/// recognizable as ours), then the ticket id and reply token joined by the
/// first `.` — ids and tokens are drawn from a nanoid alphabet that never
/// contains `.`, so splitting on the first occurrence is unambiguous.
pub fn parse_reply_tag(tag: &str) -> Option<ReplyTag> {
    let rest = tag.strip_prefix('t')?;
    let (ticket_id, reply_token) = rest.split_once('.')?;
    if ticket_id.is_empty() || reply_token.is_empty() {
        return None;
    }
    Some(ReplyTag {
        ticket_id: ticket_id.to_string(),
        reply_token: reply_token.to_string(),
    })
}

/// Every `ReplyTag` found across a batch of recipients (typically To + Cc),
/// in order, deduplicated by `(ticket_id, reply_token)` — a message can
/// legitimately be addressed to the same `+tag` recipient twice (e.g. once
/// in To and once in Cc through a mailing-list quirk), and that must not
/// produce two identical candidates to try.
pub fn extract_reply_tags(recipients: &[String]) -> Vec<ReplyTag> {
    let mut out: Vec<ReplyTag> = Vec::new();
    for raw in recipients {
        let n = super::routing::normalize_recipient(raw);
        let Some(tag_str) = n.tag else { continue };
        let Some(tag) = parse_reply_tag(&tag_str) else {
            continue;
        };
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

/// Constant-time byte comparison — the security boundary for [`ReplyTag`]
/// verification. Two tokens of different length are unequal, checked with a
/// cheap, fixed-cost length comparison first (not a secret-dependent early
/// return: every reply token this project mints is a fixed 16 characters, so
/// the length check itself leaks nothing an attacker doesn't already know
/// about the *format*); every byte position is then compared regardless of
/// where the first difference falls, so the comparison itself takes the
/// same time for a near-miss as for a completely wrong guess.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// One `Message-ID`-shaped token extracted from `In-Reply-To`/`References`.
/// Candidate ids to look up via `rfc_message_id-index`, most-likely-parent
/// first: `In-Reply-To` (the direct parent, per RFC 5322 — tried first),
/// then `References` read newest-to-oldest (a mail client appends to the
/// *end* of `References` as a thread grows, so the last entry is the
/// nearest ancestor after `In-Reply-To` itself), deduplicated in order.
pub fn candidate_message_ids(in_reply_to: Option<&str>, references: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(irt) = in_reply_to {
        let irt = irt.trim();
        if !irt.is_empty() {
            out.push(irt.to_string());
        }
    }
    if let Some(refs) = references {
        for id in refs.split_whitespace().rev() {
            let id = id.trim();
            if !id.is_empty() && !out.iter().any(|x| x == id) {
                out.push(id.to_string());
            }
        }
    }
    out
}

/// The complete, pure set of candidates to try, in priority order, for
/// resolving an inbound message to an existing ticket. `inbound::pipeline`
/// walks `reply_tags`, then `message_ids`, then `subject_number` against the
/// database — the first successful lookup wins; no match at all means "open
/// a new ticket".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolutionPlan {
    pub reply_tags: Vec<ReplyTag>,
    pub message_ids: Vec<String>,
    /// The bare ticket number parsed out of `[#{slug}-{number}]` — the slug
    /// itself is not part of the lookup key (`instance_number-index` is
    /// keyed on `{instance_id}#{number}`, and the instance is already known
    /// from address routing by the time this is used), so it's discarded
    /// here. See `inbound::pipeline`'s doc comment for why this makes the
    /// subject-tag path immune to the cross-tenant concern the other two
    /// paths need an explicit check for.
    pub subject_number: Option<u64>,
}

/// Build the plan described above from a parsed message's relevant fields.
pub fn build_resolution_plan(
    recipients: &[String],
    in_reply_to: Option<&str>,
    references: Option<&str>,
    subject: &str,
) -> ResolutionPlan {
    ResolutionPlan {
        reply_tags: extract_reply_tags(recipients),
        message_ids: candidate_message_ids(in_reply_to, references),
        subject_number: crate::outbound::parse_subject_tag(subject).map(|(_slug, number)| number),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_reply_tag ──────────────────────────────────────────────────

    #[test]
    fn parses_a_well_formed_tag() {
        let tag = parse_reply_tag("tAbC123XyZ987.reply0123456789tok").unwrap();
        assert_eq!(tag.ticket_id, "AbC123XyZ987");
        assert_eq!(tag.reply_token, "reply0123456789tok");
    }

    #[test]
    fn rejects_a_tag_missing_the_t_marker() {
        assert_eq!(parse_reply_tag("AbC123XyZ987.tok"), None);
    }

    #[test]
    fn rejects_a_tag_missing_the_dot_separator() {
        assert_eq!(parse_reply_tag("tAbC123XyZ987tok"), None);
    }

    #[test]
    fn rejects_a_tag_with_an_empty_ticket_id() {
        assert_eq!(parse_reply_tag("t.tok"), None);
    }

    #[test]
    fn rejects_a_tag_with_an_empty_token() {
        assert_eq!(parse_reply_tag("tAbC123XyZ987."), None);
    }

    #[test]
    fn rejects_a_bare_t() {
        assert_eq!(parse_reply_tag("t"), None);
    }

    #[test]
    fn rejects_an_empty_tag() {
        assert_eq!(parse_reply_tag(""), None);
    }

    #[test]
    fn only_the_first_dot_splits_ticket_id_from_token() {
        // Reply tokens are base64url-derived and never contain '.', but this
        // pins the split behaviour regardless.
        let tag = parse_reply_tag("tabc.def.ghi").unwrap();
        assert_eq!(tag.ticket_id, "abc");
        assert_eq!(tag.reply_token, "def.ghi");
    }

    // ── extract_reply_tags ───────────────────────────────────────────────

    #[test]
    fn extracts_a_tag_from_a_recipient_list() {
        let recipients =
            vec!["support+tAbC123XyZ987.tok12345678901234@acme.microticket.test".to_string()];
        let tags = extract_reply_tags(&recipients);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].ticket_id, "AbC123XyZ987");
    }

    #[test]
    fn ignores_recipients_with_no_tag_or_an_unparseable_one() {
        let recipients = vec![
            "support@acme.microticket.test".to_string(),
            "support+other-tag@acme.microticket.test".to_string(),
        ];
        assert!(extract_reply_tags(&recipients).is_empty());
    }

    #[test]
    fn dedupes_the_same_tag_seen_twice() {
        let addr = "support+tAbC123XyZ987.tok12345678901234@acme.microticket.test".to_string();
        let recipients = vec![addr.clone(), addr];
        assert_eq!(extract_reply_tags(&recipients).len(), 1);
    }

    #[test]
    fn preserves_recipient_order() {
        let recipients = vec![
            "support+tTICKETB0001.tokenB123456789012@acme.microticket.test".to_string(),
            "support+tTICKETA0001.tokenA123456789012@acme.microticket.test".to_string(),
        ];
        let tags = extract_reply_tags(&recipients);
        assert_eq!(tags[0].ticket_id, "TICKETB0001");
        assert_eq!(tags[1].ticket_id, "TICKETA0001");
    }

    // ── constant_time_eq ─────────────────────────────────────────────────

    #[test]
    fn constant_time_eq_matches_equal_slices() {
        assert!(constant_time_eq(b"abc123", b"abc123"));
    }

    #[test]
    fn constant_time_eq_rejects_different_content() {
        assert!(!constant_time_eq(b"abc123", b"abc124"));
    }

    #[test]
    fn constant_time_eq_rejects_different_length() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
    }

    #[test]
    fn constant_time_eq_treats_empty_as_equal_to_empty() {
        assert!(constant_time_eq(b"", b""));
    }

    // ── candidate_message_ids ────────────────────────────────────────────

    #[test]
    fn candidate_ids_start_with_in_reply_to() {
        let ids = candidate_message_ids(Some("<a@x>"), None);
        assert_eq!(ids, vec!["<a@x>".to_string()]);
    }

    #[test]
    fn candidate_ids_include_references_newest_first() {
        let ids = candidate_message_ids(None, Some("<a@x> <b@x> <c@x>"));
        assert_eq!(
            ids,
            vec![
                "<c@x>".to_string(),
                "<b@x>".to_string(),
                "<a@x>".to_string()
            ]
        );
    }

    #[test]
    fn candidate_ids_combine_in_reply_to_then_references_deduplicated() {
        let ids = candidate_message_ids(Some("<c@x>"), Some("<a@x> <b@x> <c@x>"));
        assert_eq!(
            ids,
            vec![
                "<c@x>".to_string(),
                "<b@x>".to_string(),
                "<a@x>".to_string()
            ],
            "In-Reply-To already covers <c@x>, so it must not repeat from References"
        );
    }

    #[test]
    fn candidate_ids_are_empty_when_both_headers_are_absent() {
        assert!(candidate_message_ids(None, None).is_empty());
    }

    #[test]
    fn candidate_ids_ignore_blank_headers() {
        assert!(candidate_message_ids(Some("   "), Some("")).is_empty());
    }

    // ── build_resolution_plan ────────────────────────────────────────────

    #[test]
    fn plan_combines_every_signal() {
        let recipients =
            vec!["support+tAbC123XyZ987.tok12345678901234@acme.microticket.test".to_string()];
        let plan = build_resolution_plan(
            &recipients,
            Some("<parent@x>"),
            Some("<older@x>"),
            "[#acme-42] Re: help",
        );
        assert_eq!(plan.reply_tags.len(), 1);
        assert_eq!(
            plan.message_ids,
            vec!["<parent@x>".to_string(), "<older@x>".to_string()]
        );
        assert_eq!(plan.subject_number, Some(42));
    }

    #[test]
    fn plan_is_empty_for_an_unrelated_message() {
        let plan = build_resolution_plan(&["nobody@example.com".to_string()], None, None, "Hello");
        assert_eq!(plan, ResolutionPlan::default());
    }
}
