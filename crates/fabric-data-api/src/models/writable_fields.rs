//! Which of a row's fields a caller is allowed to write.

use fabric_connector::{FieldName, IsolationModel};

use crate::models::discriminator::discriminator_column;
use crate::ResourceDefinition;

/// The two rules that decide whether a caller may write to a field.
///
/// The write-path mirror of [`VisibleFields`](super::VisibleFields), built the
/// same way on purpose — the two were *asymmetric*, and the asymmetry was the
/// defect:
///
/// 1. **The catalogue's allowlist**, via [`ResourceDefinition::permits_field`].
///    Identical to the read side, and always was.
/// 2. **The tenant discriminator**, matched *case-insensitively*. This is the
///    rule the write path did not have.
///
/// # What was missing
///
/// [`VisibleFields`](super::VisibleFields) treats the discriminator as
/// un-nameable whatever the catalogue says, so that "a typo cannot create the
/// leak". The write path had no equivalent. It checked
/// [`ResourceDefinition::permits_field`] and then leaned on
/// [`MutationSpec::for_target`](fabric_connector::MutationSpec::for_target)
/// overwriting the discriminator by exact string. `FieldName` compares
/// case-sensitively, so a row carrying `TENANT_KEY: "tenant-999"` reached the
/// connector *beside* the correct `tenant_key`, not instead of it.
///
/// # Why close it when it is not exploitable
///
/// It is not exploitable, and this is deliberately not presented as one. The
/// case-variant is always an *extra* key and never a replacement, so the
/// correct stamp is present on every row; and both plausible PostgreSQL
/// receivers — a `jsonb` argument keyed by column name, and a column list built
/// from the row's keys — match case-sensitively, so the variant either does
/// nothing or fails as an unknown column.
///
/// The reason to close it anyway is that the previous sentence is a claim about
/// *backend collation*, and nothing in this crate can enforce it. The read path
/// depends on no such claim. The write path should not either, and now does
/// not: rejecting a case-insensitive match makes the guarantee structural on
/// this side of the boundary, where it is testable.
///
/// # Why the refusal says "unknown field"
///
/// Rejecting `TENANT_KEY` reuses the message any other unwritable field gets.
/// Naming it as a discriminator would tell an application how it is separated
/// from other tenants, which §26 keeps from it — the same reason the read path
/// hides the column rather than explaining it.
pub(crate) struct WritableFields<'a> {
    resource: &'a ResourceDefinition,
    discriminator: Option<&'a FieldName>,
}

impl<'a> WritableFields<'a> {
    /// The write rules for one prepared operation.
    pub(crate) const fn new(resource: &'a ResourceDefinition, isolation: &'a IsolationModel) -> Self {
        Self {
            resource,
            discriminator: discriminator_column(isolation),
        }
    }

    /// Whether a caller may set this field.
    pub(crate) fn permits(&self, field: &FieldName) -> bool {
        !self.names_discriminator(field) && self.resource.permits_field(field)
    }

    /// Whether a name is the discriminator column under any casing.
    ///
    /// ASCII-only comparison is complete here, not a shortcut: every
    /// [`FieldName`] is parsed by `fabric_core::naming::parse_identifier`,
    /// which admits only ASCII letters, digits, hyphens and underscores. There
    /// is no non-ASCII casing for this to miss.
    fn names_discriminator(&self, field: &FieldName) -> bool {
        self.discriminator
            .is_some_and(|column| column.as_str().eq_ignore_ascii_case(field.as_str()))
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
    fn a_dedicated_database_restricts_nothing_beyond_the_catalogue_allowlist() {
        let open = open();
        let writable = WritableFields::new(&open, &IsolationModel::Database);

        assert!(writable.permits(&field("name")));
        // No discriminator exists on this placement, so the name is ordinary.
        assert!(writable.permits(&field("tenant_key")));
    }

    #[test]
    fn a_shared_table_refuses_the_discriminator_even_with_no_allowlist() {
        let open = open();
        let shared = shared();
        let writable = WritableFields::new(&open, &shared);

        assert!(writable.permits(&field("name")));
        assert!(!writable.permits(&field("tenant_key")));
    }

    #[test]
    fn a_case_variant_of_the_discriminator_is_refused_too() {
        // The defect: these used to pass `permits_field` and ride to the
        // connector beside the correct stamp.
        let open = open();
        let shared = shared();
        let writable = WritableFields::new(&open, &shared);

        assert!(!writable.permits(&field("TENANT_KEY")));
        assert!(!writable.permits(&field("Tenant_Key")));
        assert!(!writable.permits(&field("tEnAnT_kEy")));
    }

    #[test]
    fn a_name_that_merely_resembles_the_discriminator_is_still_writable() {
        // The rule is case, not prefix: narrowing it further would start
        // refusing ordinary columns.
        let open = open();
        let shared = shared();
        let writable = WritableFields::new(&open, &shared);

        assert!(writable.permits(&field("tenant_keys")));
        assert!(writable.permits(&field("tenant")));
    }

    #[test]
    fn a_catalogue_cannot_opt_back_into_writing_the_discriminator() {
        let listed = resource(
            r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","tenant_key"]}"#,
        );
        let shared = shared();
        let writable = WritableFields::new(&listed, &shared);

        assert!(writable.permits(&field("id")));
        assert!(!writable.permits(&field("tenant_key")));
        assert!(!writable.permits(&field("TENANT_KEY")));
    }

    #[test]
    fn both_rules_apply_together() {
        let restricted = resource(
            r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#,
        );
        let shared = shared();
        let writable = WritableFields::new(&restricted, &shared);

        assert!(writable.permits(&field("name")));
        assert!(!writable.permits(&field("salary")));
        assert!(!writable.permits(&field("tenant_key")));
    }
}
