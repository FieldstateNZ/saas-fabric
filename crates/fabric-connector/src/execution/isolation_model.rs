//! How one tenant's rows are kept apart from another's.

use serde_json::Value;

use crate::{ComparisonOperator, FieldName, Filter, SchemaName};

/// The isolation model in force for a tenant's data binding.
///
/// Specification §18 requires the runtime to support several of these, and §26
/// requires applications to be unaware of which one they are getting. This enum
/// is where that difference is confined.
///
/// # The dangerous variant
///
/// [`Self::Discriminator`] is the one to read carefully. With a dedicated
/// database or a per-tenant schema, isolation is enforced by the *connection* —
/// a query simply cannot see another tenant's rows. With a discriminator, every
/// tenant's rows sit in one table and isolation exists only because the
/// platform adds a predicate. Forget the predicate on one code path and the
/// query returns every tenant's data, with no error to notice.
///
/// [`Self::tenant_predicate`] exists so there is exactly one place that
/// predicate is produced, and [`crate::QuerySpec::for_target`] applies it
/// unconditionally so no caller can omit it.
///
/// # Unknown fields are rejected
///
/// `{"kind": "database", "column": "tenant_key", "value": "tenant-482"}` used
/// to parse as [`Self::Database`], discarding the two fields that were the
/// entire isolation mechanism: the operator believes they configured a
/// discriminator, and what they got produces no predicate at all.
///
/// `deny_unknown_fields` alone does not close this — serde applies it only to
/// variants that have fields, and [`Self::Database`] has none. Deserialisation
/// runs through a private mirror type; `execution/tagged_documents.rs` carries
/// the argument.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(from = "super::tagged_documents::IsolationModelDocument")]
pub enum IsolationModel {
    /// The connection reaches only this tenant's database.
    ///
    /// Isolation is total and needs no help from the platform.
    Database,

    /// Tenants share a database; each has its own schema.
    ///
    /// A **deferred capability** (ADR 0006). Isolation is intended to come from
    /// qualifying collection references with the schema, and nothing does that
    /// yet — see [`Self::schema`]. On the dedicated DataSource this variant is
    /// restricted to, it behaves as [`Self::Database`] does.
    Schema {
        /// The tenant's schema.
        schema: SchemaName,
    },

    /// Tenants share tables; rows carry a discriminator column.
    ///
    /// Isolation exists **only** because the platform adds a predicate to every
    /// operation.
    Discriminator {
        /// The column holding the tenant discriminator.
        column: FieldName,
        /// This tenant's value in that column.
        ///
        /// A string rather than a `TenantId` because a shared table may key on
        /// an internal surrogate (`tenant-482`) rather than the tenant's public
        /// identifier — the specification's own example does exactly that.
        value: String,
    },
}

impl IsolationModel {
    /// The predicate that must be present on every operation for this model.
    ///
    /// Returns `None` for [`Self::Database`] and [`Self::Schema`], where
    /// isolation is structural. Returns the discriminator equality for
    /// [`Self::Discriminator`].
    ///
    /// # "Structural" is a precondition, not a reassurance
    ///
    /// `None` here means *this type contributes nothing to the tenant
    /// boundary* — the separation has to come from the connection reaching a
    /// different database or schema. That is only true where the connection
    /// actually differs per tenant, and a
    /// `DataSource` carries exactly one connection shared by everyone bound
    /// to it.
    ///
    /// So these two variants are safe only on a DataSource that is not
    /// declared shared. `fabric-tenant-runtime`'s resolver enforces that and
    /// refuses the combination; ADR 0006 records why, and what it cost to
    /// find out. Nothing in *this* crate can check it — placement is not
    /// visible from here, which is exactly why the check lives one layer up.
    #[must_use]
    pub fn tenant_predicate(&self) -> Option<Filter> {
        match self {
            Self::Database | Self::Schema { .. } => None,
            Self::Discriminator { column, value } => Some(Filter::Compare {
                field: column.clone(),
                operator: ComparisonOperator::Equal,
                value: Value::String(value.clone()),
            }),
        }
    }

    /// The tenant's schema, for the day [`Self::Schema`] is implemented.
    ///
    /// # This accessor has no production callers, and that is not an oversight
    ///
    /// Nothing in this workspace qualifies a collection reference with a
    /// schema. `QuerySpec::for_target` deliberately does not rewrite collection
    /// names, and the one connector implementation
    /// (`fabric-connector-ndc`) never asks for this — it names collections
    /// exactly as the connector's own schema document does.
    ///
    /// [`Self::Schema`] is a **deferred capability**: §18 requires the model
    /// and ADR 0006 records the decision to keep the variant while it does
    /// nothing, because deleting it would erase the reason
    /// `fabric-tenant-runtime` refuses to place it on a shared DataSource. On a
    /// dedicated DataSource it behaves exactly like [`Self::Database`] —
    /// isolation comes from the connection, and this value is unused.
    ///
    /// So read this as the seam that capability will be consumed through, not
    /// as something already load-bearing. Do not add a caller that assumes it
    /// enforces anything; whatever eventually reads it will need the
    /// interpolation-safety analysis that has not been done yet.
    #[must_use]
    pub const fn schema(&self) -> Option<&SchemaName> {
        match self {
            Self::Schema { schema } => Some(schema),
            Self::Database | Self::Discriminator { .. } => None,
        }
    }

    /// A short label for telemetry (§29).
    #[must_use]
    pub const fn telemetry_label(&self) -> &'static str {
        match self {
            Self::Database => "database",
            Self::Schema { .. } => "schema",
            Self::Discriminator { .. } => "discriminator",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dedicated_database_needs_no_predicate() {
        assert!(IsolationModel::Database.tenant_predicate().is_none());
    }

    #[test]
    fn a_per_tenant_schema_isolates_structurally_not_by_predicate() {
        let model = IsolationModel::Schema {
            schema: SchemaName::try_new("acme").unwrap(),
        };

        assert!(model.tenant_predicate().is_none());
        assert_eq!(model.schema().unwrap().as_str(), "acme");
    }

    #[test]
    fn a_discriminator_produces_an_equality_predicate_on_its_column() {
        let model = IsolationModel::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "tenant-482".to_owned(),
        };

        let Some(Filter::Compare {
            field,
            operator,
            value,
        }) = model.tenant_predicate()
        else {
            panic!("a discriminator model must produce a predicate");
        };

        assert_eq!(field.as_str(), "tenant_key");
        assert_eq!(operator, ComparisonOperator::Equal);
        assert_eq!(value, Value::String("tenant-482".to_owned()));
    }

    #[test]
    fn a_discriminators_fields_under_a_database_kind_are_rejected_not_dropped() {
        // The operator who thinks they configured discriminator isolation and
        // did not. This used to parse as `Database` — no predicate, no error.
        let error = serde_json::from_str::<IsolationModel>(
            r#"{"kind": "database", "column": "tenant_key", "value": "tenant-482"}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("column"), "{error}");
    }

    #[test]
    fn a_misspelled_field_on_a_discriminator_is_rejected() {
        // `colum` rather than `column`. Without the deny, this was a missing
        // *required* field and already failed — but the same typo on an
        // optional-looking field would not have, and the rule should not
        // depend on which field was fat-fingered.
        let error = serde_json::from_str::<IsolationModel>(
            r#"{"kind": "discriminator", "column": "tenant_key", "value": "t", "schema": "acme"}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("schema"), "{error}");
    }

    #[test]
    fn the_documents_the_platform_actually_ships_still_parse() {
        // The deny must reject only surplus, never anything legitimate.
        for document in [
            r#"{"kind": "database"}"#,
            r#"{"kind": "schema", "schema": "acme"}"#,
            r#"{"kind": "discriminator", "column": "tenant_key", "value": "tenant-482"}"#,
        ] {
            assert!(
                serde_json::from_str::<IsolationModel>(document).is_ok(),
                "{document}"
            );
        }
    }

    #[test]
    fn every_variant_survives_a_round_trip() {
        // `deny_unknown_fields` on an internally tagged enum is only safe if
        // this crate's own output is still readable by it — a `Serialize` that
        // emitted anything the `Deserialize` now rejects would make the type
        // unable to read itself.
        for model in [
            IsolationModel::Database,
            IsolationModel::Schema {
                schema: SchemaName::try_new("acme").unwrap(),
            },
            IsolationModel::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: "tenant-482".to_owned(),
            },
        ] {
            let json = serde_json::to_string(&model).unwrap();
            let parsed: IsolationModel = serde_json::from_str(&json).unwrap();

            assert_eq!(parsed, model, "{json}");
        }
    }
}
