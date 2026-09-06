//! Bounding a token-derived value before it reaches a log line.
//!
//! Its own file because the rule is a policy, not a one-off: every value that
//! originates inside a bearer token is attacker-controlled, and logging it
//! unsanitised risks log injection from a control character and unbounded
//! storage from a crafted value. This is the one place that risk is closed,
//! so every call site routes through it rather than reasoning about the
//! value's shape itself.

use std::fmt;

/// The maximum number of bytes of a sanitised value that reach a log line.
const MAX_BYTES: usize = 128;

/// A token-derived value, bounded and made safe to put on a log line.
///
/// A struct rather than a `String` because neither the bound firing nor the
/// filter firing is visible in what is left: an issuer truncated at 128 bytes
/// and an issuer that is genuinely 128 bytes long look identical on the line,
/// and so do a token that carried no issuer and one whose issuer this refused
/// to print. An operator chasing registry drift needs to know which they are
/// reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitised {
    /// What reaches the log.
    pub value: String,

    /// Whether the 128-byte bound cut the value short.
    pub truncated: bool,

    /// Whether the filter dropped at least one character on the way.
    ///
    /// The case this exists for is a value made entirely of characters the
    /// filter removes: it reaches the line as an empty string with
    /// `truncated: false`, which reads as "the token carried nothing" when
    /// what happened is "the token carried something this refused to print".
    /// The two want different investigations.
    pub filtered: bool,
}

impl fmt::Display for Sanitised {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.value)
    }
}

/// Keeps the printable ASCII of a value and bounds it to 128 bytes.
///
/// This is bounding, not redaction: the value still reaches the log,
/// deliberately, because seeing it is the point — a burst of one
/// `issuer_offered` value is what makes registry drift visible. What this
/// refuses is the log stream itself becoming an attack surface.
///
/// # Why printable ASCII and not "not a control character"
///
/// The values this bounds are URIs and identifier-parse messages, and an
/// issuer is an RFC 3986 URI, whose grammar is ASCII throughout. So keeping
/// only `is_ascii_graphic` plus the space loses nothing a legitimate value
/// carries, and it closes three families a control-character test does not:
/// the Unicode line terminators U+2028 and U+2029, which several log viewers
/// and JSON consumers break a line on; the `Cf` format characters, U+202E
/// among them, which reorder the rest of the line as it is displayed so a
/// record can be made to read as something it does not say; and every
/// homoglyph that would make one issuer look like another in a dashboard.
///
/// Truncation happens on a character boundary, which after the filter is a
/// byte boundary — the filter is applied first anyway, so nothing depends on
/// the order.
///
/// A value can report both flags. What lies beyond the bound is never
/// examined, so a character the filter would have removed after truncation is
/// not counted in `filtered` — the bound is where reading stopped, and
/// `truncated` is the field that says so.
#[must_use]
pub fn sanitise(value: &str) -> Sanitised {
    let mut result = String::new();
    let mut truncated = false;
    let mut filtered = false;

    for character in value.chars() {
        if !(character.is_ascii_graphic() || character == ' ') {
            filtered = true;
            continue;
        }

        if result.len() + character.len_utf8() > MAX_BYTES {
            truncated = true;
            break;
        }

        result.push(character);
    }

    Sanitised {
        value: result,
        truncated,
        filtered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_value_passes_through_unchanged() {
        let sanitised = sanitise("https://id.example.com/realms/acme");

        assert_eq!(sanitised.value, "https://id.example.com/realms/acme");
        assert!(!sanitised.truncated);
        assert!(!sanitised.filtered);
    }

    #[test]
    fn a_value_the_filter_touched_says_so() {
        // The distinction the flag exists for. Without it these two lines are
        // `issuer_offered=acme` either way, and only one of them is a token
        // trying to write a second record.
        assert!(sanitise("acme\nSet-Cookie: evil=1").filtered);
        assert!(!sanitise("acme").filtered);
    }

    #[test]
    fn a_value_can_be_both_truncated_and_filtered() {
        let sanitised = sanitise(&format!("é{}", "a".repeat(500)));

        assert!(sanitised.truncated);
        assert!(sanitised.filtered);
    }

    #[test]
    fn control_characters_are_stripped() {
        assert_eq!(
            sanitise("acme\nSet-Cookie: evil=1").value,
            "acmeSet-Cookie: evil=1"
        );
        assert_eq!(sanitise("acme\r\nX-Injected: true").value, "acmeX-Injected: true");
    }

    #[test]
    fn a_right_to_left_override_is_stripped() {
        // U+202E is a `Cf` format character, not a control character, so a
        // `is_control` filter keeps it. It reverses the display order of
        // everything after it, which is how a log line is made to read as an
        // issuer it does not name.
        assert_eq!(
            sanitise("acme\u{202e}moc.live//:sptth").value,
            "acmemoc.live//:sptth"
        );
    }

    #[test]
    fn the_unicode_line_terminators_are_stripped() {
        // Neither is a control character, and both end a line for enough log
        // viewers and JSON consumers to be a second injection surface.
        assert_eq!(sanitise("acme\u{2028}next").value, "acmenext");
        assert_eq!(sanitise("acme\u{2029}next").value, "acmenext");
    }

    #[test]
    fn a_value_longer_than_the_bound_is_truncated_and_says_so() {
        let long = "a".repeat(500);

        let sanitised = sanitise(&long);

        assert_eq!(sanitised.value.len(), MAX_BYTES);
        assert!(sanitised.truncated);
    }

    #[test]
    fn a_value_exactly_at_the_bound_is_not_reported_as_truncated() {
        // The distinction the flag exists for: this and the case above are
        // indistinguishable on the line without it.
        let sanitised = sanitise(&"a".repeat(MAX_BYTES));

        assert_eq!(sanitised.value.len(), MAX_BYTES);
        assert!(!sanitised.truncated);
    }

    #[test]
    fn a_non_ascii_value_reaches_the_log_as_nothing_rather_than_as_itself() {
        // Deliberate. An issuer is an RFC 3986 URI, so a value made entirely
        // of non-ASCII is not a truncated issuer, it is not an issuer.
        let sanitised = sanitise("é".repeat(200).as_str());

        assert!(sanitised.value.is_empty());
        assert!(!sanitised.truncated);
        // And the line says which of the two empty values it is: nothing
        // arrived, or everything that arrived was refused.
        assert!(sanitised.filtered);
    }

    #[test]
    fn an_empty_value_stays_empty_and_reports_neither_flag() {
        let sanitised = sanitise("");

        assert_eq!(sanitised.value, "");
        assert!(!sanitised.truncated);
        assert!(!sanitised.filtered);
    }
}
