//! Which of a row's fields this request is allowed to disclose.

use fabric_connector::{FieldName, IsolationModel};

use crate::models::discriminator::discriminator_column;
use crate::ResourceDefinition;

/// The two rules that decide whether a field may appear in a response.
///
/// They are unrelated in origin and both have to hold, which is exactly why
/// they are carried together rather than checked in two places:
///
/// 1. **The catalogue's allowlist.** `queryable_fields`, applied by
///    [`ResourceDefinition::permits_field`]. A policy an operator chooses per
///    resource, and empty means "nothing hidden".
/// 2. **The tenant discriminator.** Not a policy at all. §26 requires an
///    application to be unaware of its isolation model, and on a shared table
///    the discriminator column is that model — its name says how the tenant is
///    separated and its value is the tenant's internal surrogate key.
///
/// The second is why rule 1 alone was not enough. It only fires on resources
/// whose catalogue entry happens to enumerate its columns, so every resource on
/// a shared DataSource that had not opted in — the common case — went on
/// returning `tenant_key` to the application. The platform knows the column
/// name from the resolved [`IsolationModel`] on every request, so it can hold
/// §26 without the catalogue's cooperation, and does.
///
/// A catalogue entry cannot opt out by listing the discriminator in
/// `queryable_fields`: rule 2 wins. There is no legitimate reason for an
/// application to read the column that exists only to hide other tenants from
/// it, and making it un-listable means a typo cannot create the leak.
pub(crate) struct VisibleFields<'a> {
    resource: &'a ResourceDefinition,
    discriminator: Option<&'a FieldName>,
}

impl<'a> VisibleFields<'a> {
    /// The visibility rules for one prepared operation.
    pub(crate) const fn new(resource: &'a ResourceDefinition, isolation: &'a IsolationModel) -> Self {
        Self {
            resource,
            discriminator: discriminator_column(isolation),
        }
    }

    /// Whether a response may carry this field.
    ///
    /// Exact comparison, where the write-path mirror
    /// ([`WritableFields`](super::WritableFields)) compares case-insensitively.
    /// The asymmetry is intended and is about direction of travel: this side
    /// filters names the *backend* produced against a projection built from
    /// validated `FieldName`s, while the write side filters names a *caller*
    /// chose. Only the caller is in a position to pick a casing on purpose.
    pub(crate) fn permits(&self, field: &FieldName) -> bool {
        self.discriminator != Some(field) && self.resource.permits_field(field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str) -> FieldName {
        FieldName::try_new(name).unwrap()
    }

    fn resource(json: &str) -> ResourceDefinition {
        serde_json::from_str(json).unwrap()
    }

    fn open() -> ResourceDefinition {
        resource(r#"{"data_source":"primary","collection":"customers"}"#)
    }

    fn shared() -> IsolationModel {
        IsolationModel::Discriminator {
            column: field("tenant_key"),
            value: "tenant-482".to_owned(),
        }
    }

    #[test]
    fn a_dedicated_database_hides_nothing_beyond_the_catalogue_allowlist() {
        let open = open();
        let visible = VisibleFields::new(&open, &IsolationModel::Database);

        assert!(visible.permits(&field("id")));
        assert!(visible.permits(&field("tenant_key")));
    }

    #[test]
    fn a_shared_table_hides_the_discriminator_even_with_no_allowlist() {
        // The case rule 1 alone missed: the common catalogue entry, which
        // enumerates nothing, on the placement where it matters most.
        let open = open();
        let shared = shared();
        let visible = VisibleFields::new(&open, &shared);

        assert!(visible.permits(&field("id")));
        assert!(!visible.permits(&field("tenant_key")));
    }

    #[test]
    fn a_catalogue_cannot_opt_back_into_disclosing_the_discriminator() {
        let listed = resource(
            r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","tenant_key"]}"#,
        );
        let shared = shared();
        let visible = VisibleFields::new(&listed, &shared);

        assert!(visible.permits(&field("id")));
        assert!(!visible.permits(&field("tenant_key")));
    }

    #[test]
    fn both_rules_apply_together() {
        let restricted = resource(
            r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#,
        );
        let shared = shared();
        let visible = VisibleFields::new(&restricted, &shared);

        assert!(visible.permits(&field("name")));
        assert!(!visible.permits(&field("salary")));
        assert!(!visible.permits(&field("tenant_key")));
    }
}
