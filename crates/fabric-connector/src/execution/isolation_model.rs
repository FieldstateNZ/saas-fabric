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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IsolationModel {
    /// The connection reaches only this tenant's database.
    ///
    /// Isolation is total and needs no help from the platform.
    Database,

    /// Tenants share a database; each has its own schema.
    ///
    /// Isolation comes from qualifying collection references with the schema.
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

    /// The schema to qualify collection references with, if any.
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
}
