//! Inbound-mail loop prevention — pure, header-based checks, unit-tested
//! exhaustively.
//!
//! Covers three of the four checks the build plan lists for step 7:
//! `Auto-Submitted` (anything but `no`), `Precedence: bulk|list|junk`, and
//! our own `X-Microticket-Loop` header. The fourth — "`From` is one of our
//! own inbound addresses" — needs a database lookup (it's a check against
//! the `inbound_address` table, potentially across every instance, not a
//! fact available from the headers alone) and so is **not** implemented
//! here; `inbound::pipeline` performs it separately, by feeding the `From`
//! address through the same `inbound::routing::resolve_instance_id` the
//! instance-resolution step already uses. See that module's doc comment for
//! why reusing routing is correct rather than duplicating its logic.

/// Why a message was dropped before it ever became a ticket. `Display`
/// gives the exact string logged at the drop site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropReason {
    /// `Auto-Submitted` present and not (case-insensitively) `no`.
    AutoSubmitted(String),
    /// `Precedence: bulk|list|junk` (case-insensitively).
    Precedence(String),
    /// Our own `X-Microticket-Loop` header is present — this message
    /// originated from this system's own outbound mail.
    OwnLoopHeader,
    /// `From` matches one of our own configured inbound addresses (checked
    /// by `inbound::pipeline`, not this module — see the module doc).
    FromOwnAddress(String),
}

impl std::fmt::Display for DropReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AutoSubmitted(v) => write!(f, "Auto-Submitted: {v}"),
            Self::Precedence(v) => write!(f, "Precedence: {v}"),
            Self::OwnLoopHeader => write!(f, "X-Microticket-Loop header present"),
            Self::FromOwnAddress(addr) => write!(f, "From ({addr}) is one of our own addresses"),
        }
    }
}

/// The subset of a parsed message's headers the structural loop checks
/// need. Built by `inbound::pipeline` from a `mail_parser::Message`, kept as
/// a separate struct so the checks themselves stay a pure function over
/// plain strings — no `mail_parser` type appears in this module's public
/// surface, which is what makes it trivially unit-testable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoopCheckHeaders<'a> {
    pub auto_submitted: Option<&'a str>,
    pub precedence: Option<&'a str>,
    pub loop_header_present: bool,
}

/// The three structural loop checks, in the order the build plan lists
/// them. Returns the *first* reason to drop, so a message triggering more
/// than one still logs a single, deterministic reason.
pub fn check_structural(headers: &LoopCheckHeaders<'_>) -> Option<DropReason> {
    if let Some(v) = headers.auto_submitted {
        let trimmed = v.trim();
        if !trimmed.eq_ignore_ascii_case("no") {
            return Some(DropReason::AutoSubmitted(trimmed.to_string()));
        }
    }
    if let Some(v) = headers.precedence {
        let lower = v.trim().to_lowercase();
        if matches!(lower.as_str(), "bulk" | "list" | "junk") {
            return Some(DropReason::Precedence(lower));
        }
    }
    if headers.loop_header_present {
        return Some(DropReason::OwnLoopHeader);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers<'a>(
        auto_submitted: Option<&'a str>,
        precedence: Option<&'a str>,
        loop_header_present: bool,
    ) -> LoopCheckHeaders<'a> {
        LoopCheckHeaders {
            auto_submitted,
            precedence,
            loop_header_present,
        }
    }

    #[test]
    fn clean_message_passes() {
        assert_eq!(check_structural(&headers(None, None, false)), None);
    }

    #[test]
    fn auto_submitted_no_passes() {
        assert_eq!(check_structural(&headers(Some("no"), None, false)), None);
        // Case-insensitive, and tolerant of surrounding whitespace.
        assert_eq!(check_structural(&headers(Some(" No "), None, false)), None);
    }

    #[test]
    fn auto_submitted_anything_else_drops() {
        for v in ["auto-generated", "auto-replied", "yes", "AUTO-REPLIED"] {
            let reason = check_structural(&headers(Some(v), None, false));
            assert!(matches!(reason, Some(DropReason::AutoSubmitted(_))), "{v}");
        }
    }

    #[test]
    fn precedence_bulk_list_junk_drop() {
        for v in ["bulk", "list", "junk", "BULK", "  List  "] {
            let reason = check_structural(&headers(None, Some(v), false));
            assert!(matches!(reason, Some(DropReason::Precedence(_))), "{v}");
        }
    }

    #[test]
    fn precedence_other_values_pass() {
        for v in ["urgent", "first-class", ""] {
            assert_eq!(
                check_structural(&headers(None, Some(v), false)),
                None,
                "{v}"
            );
        }
    }

    #[test]
    fn own_loop_header_drops() {
        assert_eq!(
            check_structural(&headers(None, None, true)),
            Some(DropReason::OwnLoopHeader)
        );
    }

    #[test]
    fn auto_submitted_takes_priority_over_the_others() {
        let reason = check_structural(&headers(Some("auto-generated"), Some("bulk"), true));
        assert!(matches!(reason, Some(DropReason::AutoSubmitted(_))));
    }

    #[test]
    fn precedence_takes_priority_over_loop_header() {
        let reason = check_structural(&headers(None, Some("bulk"), true));
        assert!(matches!(reason, Some(DropReason::Precedence(_))));
    }

    #[test]
    fn display_strings_are_stable_and_informative() {
        assert_eq!(
            DropReason::AutoSubmitted("auto-generated".into()).to_string(),
            "Auto-Submitted: auto-generated"
        );
        assert_eq!(
            DropReason::Precedence("bulk".into()).to_string(),
            "Precedence: bulk"
        );
        assert_eq!(
            DropReason::OwnLoopHeader.to_string(),
            "X-Microticket-Loop header present"
        );
        assert_eq!(
            DropReason::FromOwnAddress("support@acme.microticket.test".into()).to_string(),
            "From (support@acme.microticket.test) is one of our own addresses"
        );
    }
}
