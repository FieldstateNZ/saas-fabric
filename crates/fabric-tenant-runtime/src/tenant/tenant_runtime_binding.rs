//! Everything one tenant currently has.

use std::collections::BTreeMap;

use fabric_connector::SecretRef;
use fabric_core::{BindingRevision, LogicalDataSourceName, TenantId};

use crate::tenant::{ConfigurationBinding, StorageBinding, TenantDataBinding};
use crate::{ConfigurationError, ResolveError};

/// The complete runtime picture for one tenant, at one revision.
///
/// This is the `TenantRuntimeBinding` of §7, and the shape follows the
/// specification's own list: data, configuration, secrets, features, storage.
///
/// # Revisions
///
/// [`Self::revision`] is what makes the lifecycle safe (§20). It only moves
/// forward, so a late-arriving update carrying an older revision is discarded
/// rather than resurrecting a retired binding, and a migration cut-over is
/// simply "publish revision N+1".
///
/// Note what does *not* bump it: anything about the DataSource this tenant sits
/// on. Resizing a pool or correcting an endpoint changes the DataSource's
/// revision and leaves every tenant record untouched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantRuntimeBinding {
    /// Which tenant this describes.
    pub tenant: TenantId,

    /// The revision of this binding (§20).
    pub revision: BindingRevision,

    /// Logical data source name to the DataSource it is bound to.
    #[serde(default)]
    pub data: BTreeMap<LogicalDataSourceName, TenantDataBinding>,

    /// Where this tenant's configuration lives.
    #[serde(default)]
    pub configuration: Option<ConfigurationBinding>,

    /// The base secret path for this tenant, such as `vault/tenants/acme`.
    #[serde(default)]
    pub secrets: Option<SecretRef>,

    /// Feature flags in force for this tenant.
    #[serde(default)]
    pub features: BTreeMap<String, bool>,

    /// Named storage areas.
    #[serde(default)]
    pub storage: BTreeMap<String, StorageBinding>,
}

impl TenantRuntimeBinding {
    /// A binding with a tenant and revision and nothing else bound yet.
    #[must_use]
    pub fn new(tenant: TenantId, revision: BindingRevision) -> Self {
        Self {
            tenant,
            revision,
            data: BTreeMap::new(),
            configuration: None,
            secrets: None,
            features: BTreeMap::new(),
            storage: BTreeMap::new(),
        }
    }

    /// Binds a logical data source name, returning the binding for chaining.
    #[must_use]
    pub fn with_data(mut self, name: LogicalDataSourceName, binding: TenantDataBinding) -> Self {
        self.data.insert(name, binding);
        self
    }

    /// Looks up a logical data source.
    ///
    /// # Errors
    ///
    /// [`ResolveError::UnboundDataSource`] if this tenant has no binding for
    /// the name. There is deliberately no fallback to another logical name —
    /// §28 forbids quietly using "the first available database".
    pub fn data_binding(&self, name: &LogicalDataSourceName) -> Result<&TenantDataBinding, ResolveError> {
        self.data
            .get(name)
            .ok_or_else(|| ResolveError::UnboundDataSource {
                tenant: self.tenant.clone(),
                logical: name.clone(),
            })
    }

    /// Whether a feature is enabled for this tenant. Unknown features are off.
    #[must_use]
    pub fn feature(&self, name: &str) -> bool {
        self.features.get(name).copied().unwrap_or(false)
    }

    /// Checks the binding is internally coherent, at load rather than at first
    /// request.
    ///
    /// The registry calls this through
    /// [`RegistryResource::validate`](crate::RegistryResource) on every apply.
    /// It stays an inherent method as well so a caller holding a bare binding
    /// can check one without importing the lifecycle trait.
    ///
    /// # Errors
    ///
    /// [`ConfigurationError`] if the tenant declares no data bindings at all,
    /// which makes every data request fail and is almost always a mistake in
    /// how the tenant was reconciled.
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if self.data.is_empty() {
            return Err(ConfigurationError::TenantHasNoDataBindings {
                tenant: self.tenant.clone(),
            });
        }

        Ok(())
    }
}
