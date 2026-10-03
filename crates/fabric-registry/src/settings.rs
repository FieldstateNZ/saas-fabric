//! What a registry is, as it was registered: where it is served, how it names
//! repositories, which realm may issue its tokens, which credential it holds
//! and for which repositories, and which addresses it may be read at (ADR 0026
//! section 5).
//!
//! # Why one value, built by kind
//!
//! Each registry kind carries rules a caller must not be able to mix: GHCR's
//! realm is GHCR's, Docker Hub names its repositories `docker.io` while
//! serving them from `registry-1.docker.io`, and only a registry an operator
//! registered is held to public addresses. A constructor per kind is where
//! those rules are fixed, so an [`OciRegistry`](crate::OciRegistry) is never
//! assembled field by field.

mod applied;
mod credential;
mod endpoint;
mod kinds;
#[cfg(test)]
mod settings_tests;

pub use credential::{Credential, RegistrySecret};

use crate::transport::Transport;

/// Which realm may issue a registry's tokens: a closed set, one rule per kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealmRule {
    /// `ghcr` and `dockerHub`: the realm is the kind's own. A challenge naming
    /// another origin is refused, naming both, and nothing is sent to it.
    Fixed {
        /// The token endpoint, e.g. `https://ghcr.io/token`.
        realm: String,
        /// The `service` every token request names, e.g. `ghcr.io`.
        service: String,
    },

    /// A `distribution` registry: the realm origin its challenge named when it
    /// was registered, or `None` when it named none (it asked for `Basic`, or
    /// for nothing). A challenge naming any other origin is refused.
    Recorded {
        /// The recorded origin, e.g. `https://auth.example.com`.
        origin: Option<String>,
    },

    /// The deployment's own registry, and a `distribution` registry while it
    /// is being registered: the origin the first challenge names is recorded,
    /// and from then on a challenge naming another is refused.
    FollowChallenge,
}

/// Which addresses a registry may be read at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressPolicy {
    /// A registry an operator registered: every connection — to its endpoint,
    /// its realm, a redirect target or a pagination link — goes to a public
    /// address, checked after name resolution, and no URL naming an IP
    /// literal is followed.
    PublicOnly,

    /// The deployment's registry, which its configuration may put on any
    /// network the deployment chooses.
    Any,
}

/// One registry, ready to be built into an [`OciRegistry`](crate::OciRegistry)
/// with [`OciRegistry::with_settings`](crate::OciRegistry::with_settings).
///
/// Built by kind — [`ghcr`](Self::ghcr), [`docker_hub`](Self::docker_hub),
/// [`distribution`](Self::distribution), [`deployment`](Self::deployment) —
/// then given a credential with [`with_credential`](Self::with_credential).
/// Its `Debug` never shows the secret.
#[derive(Debug, Clone)]
pub struct RegistrySettings {
    /// Where it is served: an origin, or for the deployment's, the configured
    /// base URL. Checked by its constructor, and parsed once more — by the
    /// same rule as every redirect — when the client is built.
    pub(crate) endpoint: String,

    /// Which configuration field the endpoint came from, for messages.
    pub(crate) field: &'static str,

    /// How its repositories are named: `docker.io`, `ghcr.io`, `host:port`.
    pub(crate) naming_host: String,

    /// Which realm may issue its tokens.
    pub(crate) realm: RealmRule,

    /// Whether a `Basic` challenge is answered: a `distribution` registry's
    /// alone, and only toward its own origin.
    pub(crate) honours_basic: bool,

    /// The credential, and the repositories it is presented for.
    pub(crate) credential: Option<Credential>,

    /// Which addresses it may be read at.
    pub(crate) address: AddressPolicy,

    /// HTTPS, or — for this crate's tests alone — plain HTTP to loopback.
    pub(crate) transport: Transport,

    /// Whether loopback counts as public: a test switch, honoured only over
    /// plain HTTP to loopback.
    pub(crate) loopback_is_public: bool,
}

impl RegistrySettings {
    /// Which realm may issue its tokens.
    #[must_use]
    pub fn realm(&self) -> &RealmRule {
        &self.realm
    }

    /// Which addresses it may be read at.
    #[must_use]
    pub fn address(&self) -> AddressPolicy {
        self.address
    }
}
