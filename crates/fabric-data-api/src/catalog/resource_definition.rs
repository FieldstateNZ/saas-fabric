//! What one logical resource is.

use fabric_connector::{CollectionName, FieldName};
use fabric_core::LogicalDataSourceName;

use crate::OperationKind;

/// One entry in the resource catalogue.
///
/// This is the platform-level definition of a logical resource — the thing §15
/// describes:
///
/// ```yaml
/// resources:
///   customers:
///     dataSource: primary
///   auditEvents:
///     dataSource: audit
/// ```
///
/// Note what is *not* here: no server, no database, no credential. The
/// definition names a **logical** data source, and which physical resource that
/// becomes is per tenant and comes from the runtime binding. That is what lets
/// `customers` mean a dedicated Azure SQL database for one tenant and a schema
/// on shared PostgreSQL for another, with one catalogue entry (§16).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDefinition {
    /// The logical data source this resource lives in — `primary`, `audit`.
    pub data_source: LogicalDataSourceName,

    /// The physical collection name the connector knows.
    ///
    /// Separate from the logical resource name so the two can diverge: a
    /// resource called `customers` may sit on a table called
    /// `customer_records`, and renaming the table should not change the API.
    pub collection: CollectionName,

    /// The field identifying a single row, for `/{id}` routes.
    #[serde(default = "default_key_field")]
    pub key_field: FieldName,

    /// Which operations are exposed for this resource.
    ///
    /// Read-only by default. A resource has to be *deliberately* made writable,
    /// which is the right default for a catalogue entry someone adds in a hurry.
    #[serde(default = "default_operations")]
    pub operations: Vec<OperationKind>,

    /// Fields callers may filter, sort, and project on.
    ///
    /// Empty means "any field the connector has". Populating it is how a
    /// resource hides columns it should not expose to filtering — and filtering
    /// is an information channel even when the column is not projected, because
    /// a caller can learn a value by narrowing a filter until rows disappear.
    #[serde(default)]
    pub queryable_fields: Vec<FieldName>,
}

/// Most collections key on `id`.
fn default_key_field() -> FieldName {
    FieldName::try_new("id").unwrap_or_else(|_| unreachable!("\"id\" is a valid field name"))
}

/// Read-only unless the catalogue says otherwise.
fn default_operations() -> Vec<OperationKind> {
    vec![OperationKind::Read, OperationKind::List]
}

impl ResourceDefinition {
    /// Whether this resource exposes an operation.
    #[must_use]
    pub fn allows(&self, operation: OperationKind) -> bool {
        self.operations.contains(&operation)
    }

    /// Whether a caller may reference a field in a filter, sort, or projection.
    ///
    /// An empty `queryable_fields` permits everything; the connector's schema is
    /// then the only constraint.
    #[must_use]
    pub fn permits_field(&self, field: &FieldName) -> bool {
        self.queryable_fields.is_empty() || self.queryable_fields.contains(field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(json: &str) -> ResourceDefinition {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_minimal_definition_is_read_only() {
        let resource = definition(r#"{"data_source":"primary","collection":"customers"}"#);

        assert!(resource.allows(OperationKind::List));
        assert!(resource.allows(OperationKind::Read));
        assert!(!resource.allows(OperationKind::Create));
        assert!(!resource.allows(OperationKind::Delete));
    }

    #[test]
    fn the_key_field_defaults_to_id() {
        let resource = definition(r#"{"data_source":"primary","collection":"customers"}"#);

        assert_eq!(resource.key_field.as_str(), "id");
    }

    #[test]
    fn an_empty_queryable_field_list_permits_any_field() {
        let resource = definition(r#"{"data_source":"primary","collection":"customers"}"#);

        assert!(resource.permits_field(&FieldName::try_new("anything").unwrap()));
    }

    #[test]
    fn a_populated_queryable_field_list_excludes_everything_else() {
        let resource = definition(
            r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#,
        );

        assert!(resource.permits_field(&FieldName::try_new("name").unwrap()));
        assert!(!resource.permits_field(&FieldName::try_new("salary").unwrap()));
    }

    #[test]
    fn a_typo_in_a_definition_is_rejected_rather_than_ignored() {
        let result = serde_json::from_str::<ResourceDefinition>(
            r#"{"data_source":"primary","collection":"customers","operatons":["create"]}"#,
        );

        assert!(result.is_err());
    }
}
