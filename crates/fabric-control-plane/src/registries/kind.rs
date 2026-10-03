//! The closed set of registry kinds.

/// What kind of registry this is, which decides where it is served, which
/// realm issues its tokens and how it names repositories (ADR 0026 section
/// 5).
///
/// # Closed, and why `distribution` is the only open one
///
/// GHCR and Docker Hub are fixed by their kind: an operator names neither
/// host nor endpoint, so neither can be pointed somewhere else under a
/// familiar name. Anything else speaking the distribution API is
/// `distribution`, at an HTTPS origin the operator gives and the realm its
/// challenge names when it is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegistryKind {
    /// GitHub's container registry: `ghcr.io`, served at `https://ghcr.io`.
    Ghcr,

    /// Docker Hub: `docker.io`, served at `https://registry-1.docker.io`.
    DockerHub,

    /// A registry running the distribution API at an origin the operator
    /// gives, named by its host.
    Distribution,
}

/// A hosted kind's host and endpoint, fixed by this platform.
pub(super) struct Hosted {
    /// How its repositories are named.
    pub(super) host: &'static str,

    /// Where it is served.
    pub(super) endpoint: &'static str,
}

impl RegistryKind {
    /// The host and endpoint a hosted kind fixes, or `None` for
    /// `distribution`, whose operator gives them.
    pub(super) const fn hosted(self) -> Option<Hosted> {
        match self {
            Self::Ghcr => Some(Hosted {
                host: "ghcr.io",
                endpoint: "https://ghcr.io",
            }),
            Self::DockerHub => Some(Hosted {
                host: "docker.io",
                endpoint: "https://registry-1.docker.io",
            }),
            Self::Distribution => None,
        }
    }

    /// The kind as the API spells it, for messages.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Ghcr => "ghcr",
            Self::DockerHub => "dockerHub",
            Self::Distribution => "distribution",
        }
    }

    /// The hosted kind that alone may register `host`, if one does:
    /// `ghcr.io` only as `ghcr`, `docker.io` only as `dockerHub`.
    pub(super) fn owning(host: &str) -> Option<Self> {
        [Self::Ghcr, Self::DockerHub]
            .into_iter()
            .find(|kind| kind.hosted().is_some_and(|hosted| hosted.host == host))
    }
}
