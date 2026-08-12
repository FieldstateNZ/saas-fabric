//! Everything one tenant currently has.

use std::collections::BTreeMap;

use fabric_connector::{ExecutionTarget, SecretRef};
use fabric_core::{BindingRevision, DataSourceName, TenantId};

use crate::{ConfigurationBinding, DataBinding, ResolveError, StorageBinding};

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
/// # Applications never see this
///
/// The physical bindings inside are internal platform detail (§7). Nothing here
/// crosses the Data API's public surface — the only thing that escapes is an
/// [`ExecutionTarget`], and that goes *downward* to a connector, never outward
/// to an application.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantRuntimeBinding {
    /// Which tenant this describes.
    pub tenant: TenantId,

    /// The revision of this binding (§20).
    pub revision: BindingRevision,

    /// Logical data source name to its physical resolution.
    #[serde(default)]
    pub data: BTreeMap<DataSourceName, DataBinding>,

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

    /// Adds a data-source binding, returning the binding for chaining.
    #[must_use]
    pub fn with_data(mut self, name: DataSourceName, binding: DataBinding) -> Self {
        self.data.insert(name, binding);
        self
    }

    /// Looks up a logical data source.
    ///
    /// # Errors
    ///
    /// [`ResolveError::UnknownDataSource`] if this tenant has no such binding.
    /// There is deliberately no fallback to another data source — §28 forbids
    /// quietly using "the first available database".
    pub fn data_source(&self, name: &DataSourceName) -> Result<&DataBinding, ResolveError> {
        self.data
            .get(name)
            .ok_or_else(|| ResolveError::UnknownDataSource {
                tenant: self.tenant.clone(),
                data_source: name.clone(),
            })
    }

    /// Produces the connector-facing target for a logical data source.
    ///
    /// This is the step that turns "tenant `acme` wants `primary`" into
    /// something executable, and it is the last point at which the platform's
    /// own vocabulary is used. Everything downstream speaks
    /// [`ExecutionTarget`].
    ///
    /// The binding's revision is stamped onto the target, so telemetry can
    /// report which revision served a request (§29) and so a target resolved
    /// from a since-replaced binding is identifiable.
    ///
    /// # Errors
    ///
    /// [`ResolveError::UnknownDataSource`] if this tenant has no such binding.
    pub fn execution_target(&self, name: &DataSourceName) -> Result<ExecutionTarget, ResolveError> {
        let binding = self.data_source(name)?;

        Ok(ExecutionTarget::new(
            self.tenant.clone(),
            self.revision,
            binding.connector.clone(),
            binding.connection.clone(),
            binding.isolation.clone(),
        ))
    }

    /// Whether a feature is enabled for this tenant. Unknown features are off.
    #[must_use]
    pub fn feature(&self, name: &str) -> bool {
        self.features.get(name).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use fabric_connector::{ConnectionName, ConnectionSelector, ConnectorId, IsolationModel};

    use super::*;

    fn tenant() -> TenantId {
        TenantId::try_new("acme").unwrap()
    }

    fn primary() -> DataSourceName {
        DataSourceName::try_new("primary").unwrap()
    }

    fn binding() -> TenantRuntimeBinding {
        TenantRuntimeBinding::new(tenant(), BindingRevision::new(42)).with_data(
            primary(),
            DataBinding {
                connector: ConnectorId::try_new("postgres-au-east").unwrap(),
                connection: ConnectionSelector::Named {
                    name: ConnectionName::try_new("acme-prod").unwrap(),
                },
                isolation: IsolationModel::Database,
            },
        )
    }

    #[test]
    fn produces_an_execution_target_stamped_with_the_binding_revision() {
        let target = binding().execution_target(&primary()).unwrap();

        assert_eq!(target.tenant(), &tenant());
        assert_eq!(target.revision(), BindingRevision::new(42));
        assert_eq!(target.connector().as_str(), "postgres-au-east");
    }

    #[test]
    fn an_undeclared_data_source_is_rejected_rather_than_falling_back() {
        // The tenant has exactly one data source. Asking for another must not
        // quietly return it — §28 forbids "the first available database".
        let audit = DataSourceName::try_new("audit").unwrap();

        assert_eq!(
            binding().execution_target(&audit).unwrap_err(),
            ResolveError::UnknownDataSource {
                tenant: tenant(),
                data_source: audit,
            }
        );
    }

    #[test]
    fn an_unknown_feature_is_off_rather_than_an_error() {
        assert!(!binding().feature("invoicing"));
    }

    #[test]
    fn round_trips_through_json() {
        let original = binding();
        let encoded = serde_json::to_string(&original).unwrap();

        assert_eq!(
            serde_json::from_str::<TenantRuntimeBinding>(&encoded).unwrap(),
            original
        );
    }
}
