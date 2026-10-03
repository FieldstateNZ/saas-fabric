//! The deployment's own registry, built from its section.

use std::sync::Arc;

use fabric_registry::OciRegistry;

use crate::config::RegistryBinding;

/// The deployment's own registry, as configuration placed it.
pub(super) struct Deployment {
    /// Where it is served.
    pub(super) base_url: String,

    /// How its repositories are named.
    pub(super) host: String,

    /// How long a call to it may take.
    pub(super) timeout_seconds: u64,

    /// Its client when no operator gave it a credential: read anonymously.
    pub(super) anonymous: Arc<OciRegistry>,
}

impl Deployment {
    /// Builds the deployment's registry from its section.
    ///
    /// # Errors
    ///
    /// A message naming the field that is wrong.
    pub(super) fn build(binding: &RegistryBinding) -> Result<Self, String> {
        let anonymous = OciRegistry::new(&binding.base_url, &binding.host, binding.http_timeout_seconds)?;
        Ok(Self {
            base_url: binding.base_url.clone(),
            host: binding.host.clone(),
            timeout_seconds: binding.http_timeout_seconds,
            anonymous: Arc::new(anonymous),
        })
    }
}
