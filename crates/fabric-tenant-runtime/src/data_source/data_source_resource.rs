//! The DataSource resource itself.

use std::collections::BTreeMap;

use fabric_connector::{ConnectionSelector, ConnectorId};
use fabric_core::{BindingRevision, DataSourceId};

use crate::data_source::{DataResidency, DataSourceCapabilities, PlacementClass, PoolSettings};
use crate::ConfigurationError;

/// A configured physical data destination, reusable across tenants.
///
/// This owns every physical and provider concern: which connector reaches it,
/// how to select the connection, how its pool is sized, where it lives, what it
/// permits. A tenant binding owns none of that — it names a `DataSource` and
/// says how that tenant is isolated within it.
///
/// # Credentials
///
/// There is no credential here, only a [`ConnectionSelector`], which is either
/// a name the connector already holds configuration for or a
/// [`SecretRef`](fabric_connector::SecretRef) (§21). A resolved secret never
/// appears in a DataSource, never appears in the file a DataSource is loaded
/// from, and never appears in telemetry (§29).
///
/// # Applications never see this
///
/// Not the type, not its fields, not its id. Everything here is placement
/// detail that §2 and §26 keep behind the Data API.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    /// This DataSource's identity, referenced by tenant bindings.
    pub id: DataSourceId,

    /// The revision of this DataSource's configuration (§20).
    ///
    /// Independent of any tenant's revision — that independence is the point.
    /// Resizing a pool bumps this and no tenant record at all.
    pub revision: BindingRevision,

    /// Which connector executes against this DataSource.
    pub connector: ConnectorId,

    /// How to select the connection within that connector.
    ///
    /// **Required, deliberately.** This used to default to
    /// [`ConnectionSelector::Default`](fabric_connector::ConnectionSelector::Default),
    /// whose own docs call it valid "only where one connector serves exactly
    /// one physical database" — so two DataSources that both said nothing were
    /// two ids, two revisions, and one database. A precondition nobody
    /// verified, reachable by omission, is not a precondition. Omission is now
    /// a deserialisation error, and the claim a stated `default` makes is
    /// checked against the rest of the snapshot by
    /// [`DestinationReuse`](crate::DestinationReuse).
    pub connection: ConnectionSelector,

    /// The service class this DataSource provides (§17).
    pub placement: PlacementClass,

    /// Where the data physically lives.
    pub residency: DataResidency,

    /// Pool sizing, applied by reconciliation to the connector (§22).
    #[serde(default)]
    pub pool: PoolSettings,

    /// What the platform permits this DataSource to be used for.
    #[serde(default)]
    pub capabilities: DataSourceCapabilities,

    /// Operator-defined labels, emitted with telemetry.
    ///
    /// For the dimensions a particular platform cares about — owning team,
    /// cost centre, maintenance window — without this type growing a field per
    /// deployment's taxonomy.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

impl DataSource {
    /// Checks the DataSource is usable, at load rather than at first request.
    ///
    /// The registry calls this through
    /// [`RegistryResource::validate`](crate::RegistryResource) on every apply,
    /// which is what makes "at load" true rather than aspirational. It stays
    /// an inherent method as well so a caller holding a bare `DataSource` —
    /// the example-state tests, chiefly — can check one without importing the
    /// lifecycle trait.
    ///
    /// # Nothing on a DataSource alone can make it unresolvable
    ///
    /// The bar this applies is deliberately narrow: **refuse a DataSource only
    /// when this process cannot turn it into an
    /// [`ExecutionTarget`](fabric_connector::ExecutionTarget).** Nothing here
    /// currently clears that bar, and saying so in one line is what the
    /// [`RegistryResource::validate`](crate::RegistryResource) contract asks a
    /// type with nothing to check to do.
    ///
    /// Field by field: `id`, `revision`, `connector` and `connection` are
    /// newtypes that were checked when they were parsed; `capabilities`
    /// defaults closed and is total either way; `residency` and `labels` are
    /// carried and reported, never branched on. `placement` *is* read on the
    /// request path, but only against a tenant's isolation model — a question
    /// about a pair, answered at resolution by
    /// [`ResolveError::IsolationNotEnforceable`](crate::ResolveError), which no
    /// check on a lone DataSource could ask.
    ///
    /// Whether `connection` collides with another DataSource's is likewise a
    /// question about the *set*, answered by
    /// [`RegistryResource::derive_set_facts`](crate::RegistryResource) when a
    /// snapshot installs. Refusing the DataSource over it would be worse than
    /// useless: a collision makes structural isolation unenforceable and
    /// leaves discriminator isolation perfectly safe, so dropping it would
    /// take working tenants offline over a fault that does not touch them.
    ///
    /// `pool` is the one field that used to be checked here, and no longer is:
    /// [`PoolSettings::validate`] carries the argument for why refusing a whole
    /// DataSource over a number this process never reads did more damage than
    /// the fault it guarded against.
    ///
    /// # Errors
    ///
    /// [`ConfigurationError::InvalidResource`] naming the offending setting,
    /// once there is a setting that genuinely qualifies.
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        Ok(())
    }

    /// A short, non-sensitive description for telemetry (§29).
    ///
    /// Includes the connection *label*, which for a secret-backed connection is
    /// the reference path — never a resolved value.
    #[must_use]
    pub fn telemetry_label(&self) -> String {
        format!(
            "{}/{}/{}",
            self.id,
            self.connector,
            self.connection.telemetry_label()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything a DataSource needs except `connection`.
    const WITHOUT_CONNECTION: &str = r#"{
        "id": "pg-01",
        "revision": 1,
        "connector": "postgres",
        "placement": "dedicated",
        "residency": {"region": "au-east"}
    }"#;

    #[test]
    fn omitting_the_connection_is_rejected_rather_than_defaulted() {
        // The whole of the omission half of the fix. Saying nothing used to
        // mean `ConnectionSelector::Default` — "the connector's one database"
        // — which is a claim about infrastructure that an operator who simply
        // did not fill the field in has not made.
        let error = serde_json::from_str::<DataSource>(WITHOUT_CONNECTION).unwrap_err();

        assert!(error.to_string().contains("connection"), "{error}");
    }

    #[test]
    fn selecting_the_default_connection_on_purpose_still_works() {
        // The variant is not deprecated, only un-defaulted. An operator whose
        // connector genuinely serves one database says so.
        let json = WITHOUT_CONNECTION.replace(
            r#""connector": "postgres","#,
            r#""connector": "postgres", "connection": {"kind": "default"},"#,
        );

        let parsed: DataSource = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.connection, ConnectionSelector::Default);
    }
}
