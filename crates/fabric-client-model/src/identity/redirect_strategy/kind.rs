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

    /// Loopback, over `http` or `https`, on any port.
    Development,

    /// A native application's own private-use URI scheme.
    ///
    /// Representable from the start so that documents do not have to change
    /// again when it lands, and refused at validation until they do — see
    /// `identity::client_rules`.
    CustomScheme(AppScheme),
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
