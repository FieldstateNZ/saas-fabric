//! Bounding a token-derived value before it reaches a log line.
//!
//! Its own file because the rule is a policy, not a one-off: every value that
//! originates inside a bearer token is attacker-controlled, and logging it
//! unsanitised risks log injection from a control character and unbounded
//! storage from a crafted value. This is the one place that risk is closed,
//! so every call site in `logging` routes through it rather than reasoning
//! about the value's shape itself.

/// The maximum number of bytes of a sanitised value that reach a log line.
const MAX_BYTES: usize = 128;

/// Strips control characters and truncates to [`MAX_BYTES`], on a character
/// boundary.
///
/// This is bounding, not redaction: the value still reaches the log,
/// deliberately, because seeing it is the point — a burst of one
/// `issuer_offered` value is what makes registry drift visible. What this
/// refuses is the log stream itself becoming an attack surface: a newline
/// forging a second log line, or a value long enough to matter for storage.
#[must_use]
pub(crate) fn sanitise(value: &str) -> String {
    let mut result = String::new();

    for character in value.chars().filter(|character| !character.is_control()) {
        if result.len() + character.len_utf8() > MAX_BYTES {
            break;
        }
        result.push(character);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_value_passes_through_unchanged() {
        assert_eq!(
            sanitise("https://id.example.com/realms/acme"),
            "https://id.example.com/realms/acme"
        );
    }

    #[test]
    fn control_characters_are_stripped() {
        assert_eq!(sanitise("acme\nSet-Cookie: evil=1"), "acmeSet-Cookie: evil=1");
        assert_eq!(sanitise("acme\r\nX-Injected: true"), "acmeX-Injected: true");
    }

    #[test]
    fn a_value_longer_than_the_bound_is_truncated() {
        let long = "a".repeat(500);

        let sanitised = sanitise(&long);

        assert_eq!(sanitised.len(), MAX_BYTES);
    }

    #[test]
    fn truncation_lands_on_a_character_boundary() {
        // Each of these is a multi-byte character, so a byte-oblivious cut
        // would produce invalid UTF-8 partway through the last one.
        let long = "é".repeat(200);

        let sanitised = sanitise(&long);

        assert!(sanitised.len() <= MAX_BYTES);
        assert!(std::str::from_utf8(sanitised.as_bytes()).is_ok());
    }

    #[test]
    fn an_empty_value_stays_empty() {
        assert_eq!(sanitise(""), "");
    }
}
