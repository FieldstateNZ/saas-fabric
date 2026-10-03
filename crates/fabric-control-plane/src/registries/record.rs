//! What this platform records about a registry.
//!
//! **No credential is in here.** A token goes to the secret partition, under
//! a name built from [`SecretId`]; what is here may be shown to an operator.

use fabric_component::Repository;

pub(crate) use crate::registries::secret_id::SecretId;
use crate::registries::{RegistryHost, RegistryKind};

/// One registry, as it was registered and proven.
///
/// Stored whole with every other in one record set, and read back strictly:
/// a field this code does not know is a record somebody else wrote, and
/// reported as unreadable rather than half-understood.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryRecord {
    /// What it is known by.
    pub(crate) host: RegistryHost,

    /// Its kind, fixed when it was registered.
    pub(crate) kind: RegistryKind,

    /// Where it is served, fixed when it was registered: an origin with no
    /// trailing `/`.
    pub(crate) endpoint: String,

    /// The origin of the realm its challenge named when it was proven, or
    /// `None` when it named none.
    #[serde(default)]
    pub(crate) realm_origin: Option<String>,

    /// Names this registry's credential in the secret partition.
    pub(crate) secret_id: SecretId,

    /// Who holds the credential, when one is held.
    #[serde(default)]
    pub(crate) credential: Option<CredentialRecord>,

    /// The operator who registered it.
    pub(crate) registered_by: String,

    /// When, in Unix seconds.
    pub(crate) registered_at: u64,

    /// Its repositories, in the order they were registered.
    #[serde(default)]
    pub(crate) repositories: Vec<RegisteredRepository>,
}

/// Who a credential belongs to, and who set it when. Never the token.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CredentialRecord {
    /// The registry account the token belongs to.
    pub(crate) username: String,

    /// The operator who set it.
    pub(crate) set_by: String,

    /// When, in Unix seconds.
    pub(crate) set_at: u64,
}

/// A repository registered under a registry, and when it was last proven.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RegisteredRepository {
    /// Its full name, on this registry's host.
    pub(crate) repository: Repository,

    /// When its tag listing last answered, in Unix seconds.
    pub(crate) proven_at: u64,
}

impl RegistryRecord {
    /// What it is known by.
    #[must_use]
    pub fn host(&self) -> &RegistryHost {
        &self.host
    }

    /// Whether `repository` is registered under it.
    pub(crate) fn holds(&self, repository: &Repository) -> bool {
        self.repositories
            .iter()
            .any(|registered| &registered.repository == repository)
    }

    /// Its repositories' names.
    pub(crate) fn repository_names(&self) -> Vec<Repository> {
        self.repositories
            .iter()
            .map(|registered| registered.repository.clone())
            .collect()
    }
}
