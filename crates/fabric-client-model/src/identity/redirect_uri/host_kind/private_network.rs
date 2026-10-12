//! The top-level domain reserved for private use, and the one test of it.
//!
//! Its own file because two callers need the same answer: the classifier,
//! which admits plain HTTP under it, and the application projection, which
//! follows a client's declared hosts to `privateNetwork` (D01-15). Two copies
//! of the suffix would be two answers waiting to disagree.

/// The top-level domain ICANN reserved for private-use applications.
///
/// Its board resolved in July 2024 to withhold `.internal` from delegation
/// permanently, for exactly this purpose: names that resolve only inside an
/// organisation. Because it will never exist in the public DNS root it cannot
/// resolve on the internet and no public certificate authority will issue for
/// it — which is what makes the plain-HTTP exception a rule rather than a
/// favour to one deployment.
const PRIVATE_TLD: &str = "internal";

/// Whether an already-lower-cased host is the private top-level domain or
/// sits under it.
///
/// A suffix test on whole labels: `corp.internal` is under it and
/// `notinternal` is not.
pub(crate) fn is_private_network(host: &str) -> bool {
    host == PRIVATE_TLD
        || host
            .strip_suffix(PRIVATE_TLD)
            .is_some_and(|rest| rest.ends_with('.'))
}

#[cfg(test)]
mod tests {
    use super::is_private_network;

    #[test]
    fn the_private_tld_and_every_name_under_it_are_private_network() {
        assert!(is_private_network("internal"));
        assert!(is_private_network("acme.lucentroot.internal"));
    }

    #[test]
    fn a_name_that_only_ends_in_the_same_letters_is_not() {
        assert!(!is_private_network("notinternal"));
        assert!(!is_private_network("acme.notinternal"));
        assert!(!is_private_network("internal.example.com"));
    }
}
