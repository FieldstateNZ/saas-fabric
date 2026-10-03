//! The hostname and identifier value rules, re-declared over
//! `fabric_core::naming`.
//!
//! # Why re-declared, not shared
//!
//! A configuration field of kind `hostname` or `identifier` has always been
//! checked with `fabric-client-model`'s `Host` and `ClientId`. Those types
//! are the control plane's own, and this crate is in neither plane, so the
//! two rules are written again here over the same `fabric_core::naming`
//! function -- as `fabric-runtime-publication` re-declares its identifiers --
//! and a test in `fabric-client-model` holds the two copies to one answer.

/// The longest a fully qualified name may be, from the DNS specification;
/// the same bound `Host` applies.
const MAX_HOSTNAME: usize = 253;

/// Whether `value` is a DNS hostname: non-empty, at most 253 bytes, and
/// every dot-separated label a DNS label -- no scheme, port, path, trailing
/// dot or wildcard. The rule `fabric-client-model`'s `Host` applies.
#[must_use]
pub fn is_hostname(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_HOSTNAME
        && value
            .split('.')
            .all(|label| fabric_core::naming::parse_dns_label("host", label).is_ok())
}

/// Whether `value` is a stable slug: one DNS label. The rule
/// `fabric-client-model`'s `ClientId` applies.
#[must_use]
pub fn is_identifier(value: &str) -> bool {
    fabric_core::naming::parse_dns_label("client id", value).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hostname_is_labels_and_nothing_else() {
        assert!(is_hostname("www.example.com"));
        assert!(!is_hostname("https://www.example.com"));
        assert!(!is_hostname("www.example.com:8443"));
        assert!(!is_hostname("www..example.com"));
        assert!(!is_hostname(""));
    }

    #[test]
    fn an_identifier_is_one_label() {
        assert!(is_identifier("acme"));
        assert!(!is_identifier("acme.example"));
        assert!(!is_identifier("Acme"));
    }
}
