//! How a redirect strategy is spelled in the document.
//!
//! Its own type, converted in **both** directions, because the strategy is
//! serialised out as well as read in: the control-plane API's identity
//! response holds application clients directly, and the document the control
//! plane writes back is rendered from the same values. A `try_from`-only type
//! would not compile there, and a hand-written `Serialize` beside a derived
//! `Deserialize` is two spellings that can drift apart.

use super::{RedirectStrategy, RedirectStrategyKind};
use crate::{AppScheme, RedirectUri};

/// The document's spelling of a `redirect` block.
///
/// Internally tagged on `strategy`, so the block reads as one mapping rather
/// than a nested single-key one. Values are camelCase because `claimedhttps`
/// is not a word and a hyphen would be a third convention beside the
/// lowercase values (`type: oidc`) and camelCase keys the document already
/// uses.
///
/// `deny_unknown_fields`, so a misspelled key is a refusal an operator sees
/// rather than a value silently ignored — the same rule the identity block
/// already applies to itself.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "strategy", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum RedirectStrategyShape {
    /// Public `https://` hosts only.
    ClaimedHttps {
        /// The callbacks the identity provider should register.
        uris: Vec<RedirectUri>,
    },

    /// `.internal` hosts, over `http` or `https`.
    PrivateNetwork {
        /// The callbacks the identity provider should register.
        uris: Vec<RedirectUri>,
    },

    /// Loopback, on any port.
    Development {
        /// The callbacks the identity provider should register.
        uris: Vec<RedirectUri>,
    },

    /// A native application's own private-use URI scheme.
    CustomScheme {
        /// The scheme the application registered with the operating system.
        scheme: AppScheme,
        /// The callbacks the identity provider should register.
        uris: Vec<RedirectUri>,
    },
}

impl From<RedirectStrategyShape> for RedirectStrategy {
    /// Structural only, and deliberately so: see [`RedirectStrategy`]'s note
    /// on why a stored document's rule breaches are reported by `validate`
    /// rather than by serde.
    fn from(shape: RedirectStrategyShape) -> Self {
        let (kind, uris) = match shape {
            RedirectStrategyShape::ClaimedHttps { uris } => (RedirectStrategyKind::ClaimedHttps, uris),
            RedirectStrategyShape::PrivateNetwork { uris } => (RedirectStrategyKind::PrivateNetwork, uris),
            RedirectStrategyShape::Development { uris } => (RedirectStrategyKind::Development, uris),
            RedirectStrategyShape::CustomScheme { scheme, uris } => {
                (RedirectStrategyKind::CustomScheme(scheme), uris)
            }
        };

        Self::from_parts(kind, uris)
    }
}

impl From<RedirectStrategy> for RedirectStrategyShape {
    fn from(strategy: RedirectStrategy) -> Self {
        let (kind, uris) = strategy.into_parts();

        match kind {
            RedirectStrategyKind::ClaimedHttps => Self::ClaimedHttps { uris },
            RedirectStrategyKind::PrivateNetwork => Self::PrivateNetwork { uris },
            RedirectStrategyKind::Development => Self::Development { uris },
            RedirectStrategyKind::CustomScheme(scheme) => Self::CustomScheme { scheme, uris },
        }
    }
}
