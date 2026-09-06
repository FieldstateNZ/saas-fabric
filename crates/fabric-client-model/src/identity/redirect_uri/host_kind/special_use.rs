//! The top-level domains the public DNS has reserved, and what each one means
//! here.
//!
//! Its own file because "reserved" is not one rule but three RFCs with two
//! different consequences, and the consequence is what the classifier needs.
//!
//! RFC 6761 §6.3 requires every name under `.localhost` to resolve to the
//! loopback interface, and Chrome and Firefox do — so `app.localhost` reaches
//! the machine the browser is already on and is a loopback near-miss, not a
//! public host. RFC 6762 gives `.local` to multicast DNS, and RFC 2606 sets
//! `.test`, `.example` and `.invalid` aside permanently.
//!
//! None of the five can ever be delegated, so nobody can register one and no
//! public certificate authority will issue for one. That is the same
//! criterion `.internal` is admitted on, reaching the opposite conclusion: a
//! private-network host is a kind of its own **because** no publicly-trusted
//! certificate can exist for it, and these are refused because a name nobody
//! can hold is a name nobody can prove they control.

use fabric_core::IdentifierError;

/// The label used in error messages when parsing fails.
const KIND: &str = "redirect uri";

/// The suffix of the top-level domain RFC 6761 §6.3 gives to loopback.
const UNDER_LOOPBACK_TLD: &str = ".localhost";

/// Every reserved top-level domain a claimed-HTTPS host may sit under, with
/// what its author is told.
///
/// A message each rather than one shared message: the RFC that reserved the
/// name is the whole evidence the refusal rests on, and an author who wants
/// to argue with it has to be able to look it up.
const RESERVED: [(&str, &str); 4] = [
    (
        "local",
        "a registered domain — .local is reserved for multicast DNS by RFC 6762 and can never be \
         registered, so a callback on it can be claimed as neither a Universal Link nor an App Link",
    ),
    (
        "test",
        "a registered domain — .test is reserved for testing by RFC 2606 and can never be \
         registered, so a callback on it can be claimed as neither a Universal Link nor an App Link",
    ),
    (
        "example",
        "a registered domain — .example is reserved for documentation by RFC 2606 and can never be \
         registered, so a callback on it can be claimed as neither a Universal Link nor an App Link",
    ),
    (
        "invalid",
        "a registered domain — .invalid is reserved by RFC 2606 for names guaranteed not to \
         resolve, so a callback on it can be claimed as neither a Universal Link nor an App Link",
    ),
];

/// Whether a host sits under `.localhost`.
///
/// Kept out of [`check`] deliberately. RFC 6761 §6.3 makes every name under
/// `.localhost` resolve to loopback, so `app.localhost` has to be refused
/// under **both** schemes — before the plain-HTTP arm, not after it — and
/// that ordering lives in [`super::reaches_loopback`], with the other
/// spellings that reach the machine the browser is already on.
pub(super) fn reaches_localhost(host: &str) -> bool {
    host.ends_with(UNDER_LOOPBACK_TLD)
}

/// Refuses a host under a top-level domain nobody can register.
///
/// Reached only for `https` on a host that is neither loopback nor
/// `.internal`, so a refusal here is about the production rule: an entitlement
/// stated against a name that can never be held, and never carry a publicly
/// trusted certificate, is an entitlement nothing can satisfy.
///
/// # Errors
///
/// Returns [`IdentifierError::Unadmitted`] naming the RFC that reserved the
/// name.
pub(super) fn check(host: &str) -> Result<(), IdentifierError> {
    for (tld, expected) in RESERVED {
        if host == tld || host.ends_with(&format!(".{tld}")) {
            return Err(IdentifierError::Unadmitted { kind: KIND, expected });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_multicast_dns_domain_is_refused_and_names_its_rfc() {
        let error = check("printer.local").unwrap_err();

        assert!(error.to_string().contains("RFC 6762"), "{error}");
        assert!(error.to_string().contains("Universal Link"), "{error}");
    }

    #[test]
    fn the_three_rfc_2606_domains_are_refused_and_name_theirs() {
        for host in ["app.test", "www.example", "nothing.invalid"] {
            let error = check(host).unwrap_err();

            assert!(error.to_string().contains("RFC 2606"), "{host}: {error}");
            assert!(error.to_string().contains("App Link"), "{host}: {error}");
        }
    }

    #[test]
    fn the_bare_reserved_name_is_refused_as_the_reserved_name() {
        // A single-label host would be refused anyway, for having one label.
        // It is refused here instead so the message names the reservation,
        // which is the reason no second label would help.
        for host in ["local", "test", "example", "invalid"] {
            assert!(check(host).is_err(), "{host}");
        }
    }

    #[test]
    fn a_reserved_name_that_is_not_the_suffix_is_an_ordinary_host() {
        // The mistake a substring test would make: every one of these is a
        // registrable name that merely contains a reserved label.
        for host in [
            "www.example.com",
            "test.example.com",
            "local.example.com",
            "invalid.co.nz",
        ] {
            assert!(check(host).is_ok(), "{host}");
        }
    }

    #[test]
    fn only_a_name_under_the_loopback_domain_reaches_localhost() {
        assert!(reaches_localhost("app.localhost"));
        assert!(reaches_localhost("a.b.localhost"));
        assert!(!reaches_localhost("localhost"));
        assert!(!reaches_localhost("notlocalhost"));
        assert!(!reaches_localhost("localhost.localdomain"));
    }
}
