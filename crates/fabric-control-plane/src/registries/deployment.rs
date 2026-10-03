//! The deployment's own registry, as configuration placed it.

/// Where the deployment reads its own images from:
/// `[platform_management.registry]`, when Platform Management is configured.
///
/// # Why an operator's registry for this host contributes so little
///
/// The deployment chose this endpoint — a mirror, perhaps, on a network only
/// it can reach — and keeps its own rules for it (ADR 0026 section 5). An
/// operator may give it a credential and repositories, and nothing else: a
/// registration for this host must name this endpoint, so a credential is
/// only ever presented where the deployment already reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentRegistry {
    /// How its repositories are named.
    pub(crate) host: String,

    /// Where it is served.
    pub(crate) endpoint: String,
}

impl DeploymentRegistry {
    /// The deployment's registry, named `host` and served at `endpoint`.
    #[must_use]
    pub fn new(host: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            endpoint: endpoint.into().trim_end_matches('/').to_owned(),
        }
    }

    /// How its repositories are named.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Where it is served.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}
