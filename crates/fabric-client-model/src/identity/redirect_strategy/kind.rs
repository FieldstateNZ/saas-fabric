//! The four kinds of client this platform can describe.

use std::fmt;

use crate::AppScheme;

/// Which kind of callback an application client is entitled to.
///
/// A closed set, and deliberately four rather than the three an obvious
/// reading would give. `PrivateNetwork` exists because this platform already
/// runs a deployment for which plain-HTTP `.internal` is the *production*
/// posture: folding it into `Development` would make every one of that
/// deployment's documents say something false about itself, and folding it
/// into `ClaimedHttps` would put a plain-HTTP URI inside the variant whose
/// whole job is to be the HTTPS rule. A closed set that cannot describe a
/// deployment we already run is not closed enough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectStrategyKind {
    /// Public `https://` hosts only. The production rule, and also what an iOS
    /// Universal Link and an Android App Link are.
    ClaimedHttps,

    /// Hosts under the ICANN-reserved `.internal` top-level domain, over
    /// `http` or `https`.
    PrivateNetwork,

    /// Loopback. Over `http`, a URI registered without a port matches any port
    /// (Keycloak compares no port for it, RFC 8252 §7.3); over `https`, and
    /// whenever a port is written, the match is exact.
    ///
    /// The asymmetry is observed rather than assumed — Keycloak 26.0.8,
    /// 2026-09-06 — and it is why there is no `:*` spelling: that one matches
    /// nothing at all.
    Development,

    /// A native application's own private-use URI scheme.
    ///
    /// Representable from the start so that documents do not have to change
    /// again when it lands, and refused at validation until they do — see
    /// `identity::client_rules`.
    CustomScheme(AppScheme),
}

impl RedirectStrategyKind {
    /// What this strategy admits, phrased for the message a refusal produces.
    ///
    /// Beside [`Display`](fmt::Display) rather than in `rules`, for the same
    /// reason: both are how a kind describes *itself* to an operator, and a
    /// refusal that names the strategy has to be able to say in the same
    /// breath what that strategy would have taken. `rules` keeps the table of
    /// which URI kinds are admitted, which is the decision; this is its
    /// wording.
    pub(crate) const fn admitted(&self) -> &'static str {
        match self {
            Self::ClaimedHttps => "only public https callbacks",
            Self::PrivateNetwork => "only .internal callbacks, over http or https",
            Self::Development => "only loopback callbacks — 127.0.0.1, ::1 or localhost",
            Self::CustomScheme(_) => "only callbacks on the scheme it declares",
        }
    }
}

impl fmt::Display for RedirectStrategyKind {
    /// The document's own spelling, so a refusal names what the operator
    /// wrote rather than what Rust calls it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ClaimedHttps => "claimedHttps",
            Self::PrivateNetwork => "privateNetwork",
            Self::Development => "development",
            Self::CustomScheme(_) => "customScheme",
        })
    }
}
