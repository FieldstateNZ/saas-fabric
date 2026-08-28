//! The tenant discriminator column, where the placement has one.

use fabric_connector::{FieldName, IsolationModel};

/// The column carrying the tenant discriminator, if this placement uses one.
///
/// Matched rather than derived from
/// [`tenant_predicate`](fabric_connector::IsolationModel::tenant_predicate):
/// that returns a `Filter`, and digging a column back out of a predicate would
/// break the moment the predicate's shape changed.
///
/// Shared by [`VisibleFields`](super::VisibleFields) and
/// [`WritableFields`](super::WritableFields) rather than written out twice. The
/// two rules differ in how they *compare* a name, which is the interesting part
/// and is documented on each; they must never differ in which column they
/// compare against, and one function is how that is guaranteed rather than
/// hoped.
pub(crate) const fn discriminator_column(isolation: &IsolationModel) -> Option<&FieldName> {
    match isolation {
        IsolationModel::Discriminator { column, .. } => Some(column),
        IsolationModel::Database | IsolationModel::Schema { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_table_reports_its_discriminator_column() {
        let isolation = IsolationModel::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "tenant-482".to_owned(),
        };

        assert_eq!(
            discriminator_column(&isolation).map(FieldName::as_str),
            Some("tenant_key")
        );
    }

    #[test]
    fn a_dedicated_placement_has_no_discriminator_column() {
        assert!(discriminator_column(&IsolationModel::Database).is_none());
    }
}
