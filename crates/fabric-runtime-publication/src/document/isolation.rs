//! How one tenant's rows are kept apart from another's, as the publisher
//! declares it.

use crate::{FieldName, SchemaName};

/// The publisher's own declaration of a tenant data binding's isolation
/// model.
///
/// Mirrors `fabric_connector::IsolationModel` — see
/// [`TenantBindingDocument`](crate::TenantBindingDocument) for why this crate
/// declares its own copy.
///
/// # Every variant is struct-shaped, including the empty one
///
/// `Database {}` looks like it should be a plain unit variant, and nothing
/// in this crate would mind if it were — but an internally tagged unit
/// variant has no field list for serde to check a surplus field against, so
/// `deny_unknown_fields` silently does nothing for it. That gap is exactly
/// what once let `{"kind":"database","column":"x","value":"y"}` parse as
/// `Database` on the runtime side, discarding an operator's discriminator
/// column with no error at all. Declaring the empty variant as `Database {}`
/// gives it a (empty) field list, so the same surplus is refused here too.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IsolationModelDocument {
    /// The connection reaches only this tenant's database.
    Database {},

    /// Tenants share a database; each has its own schema.
    Schema {
        /// The tenant's schema.
        schema: SchemaName,
    },

    /// Tenants share tables; rows carry a discriminator column.
    Discriminator {
        /// The column holding the tenant discriminator.
        column: FieldName,
        /// This tenant's value in that column.
        value: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_round_trips_through_json() {
        for model in [
            IsolationModelDocument::Database {},
            IsolationModelDocument::Schema {
                schema: SchemaName::try_new("acme").unwrap(),
            },
            IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "tenant-482".to_owned(),
            },
        ] {
            let json = serde_json::to_string(&model).unwrap();
            let parsed: IsolationModelDocument = serde_json::from_str(&json).unwrap();

            assert_eq!(parsed, model, "{json}");
        }
    }

    #[test]
    fn a_discriminators_fields_under_a_database_kind_are_rejected_not_dropped() {
        let error = serde_json::from_str::<IsolationModelDocument>(
            r#"{"kind": "database", "column": "tenant_key", "value": "tenant-482"}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("column"), "{error}");
    }
}
