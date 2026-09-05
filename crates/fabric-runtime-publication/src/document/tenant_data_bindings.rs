//! The non-empty set of a tenant's logical data source bindings.

use std::collections::btree_map;
use std::collections::BTreeMap;

use fabric_core::LogicalDataSourceName;

use crate::TenantDataBindingDocument;

/// A tenant's data source bindings — the value of
/// [`TenantBindingDocument::data`](crate::TenantBindingDocument::data).
///
/// # Non-empty by construction, permissive on deserialisation
///
/// ADR 0018's wire contract table marks `data` "yes, non-empty": the
/// consumer's own `validate` rejects an empty map outright and drops the
/// whole binding, retaining the last held copy instead (`merge.rs:41-60`). A
/// producer that builds an empty map is therefore building a binding the
/// runtime will silently discard on arrival, which is never what publishing
/// one is meant to do — so [`Self::try_new`] refuses it before it can be
/// assembled into a [`crate::TenantBindingDocument`].
///
/// Deserialisation stays permissive on purpose: an absent or empty `data` key
/// on the wire is the consumer's problem to police, not a second occasion for
/// this crate to refuse it, and a document already published — or the held
/// copy a publication reads back to validate against — must keep parsing
/// regardless of what it contains. `Self::empty_for_deserialisation` is what
/// `#[serde(default)]` calls on an absent field; it is deliberately not
/// [`Default`], and deliberately not `pub`, so nothing outside this crate's
/// own deserialisation path can reach an empty value without going through
/// the checked constructor.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct TenantDataBindings(BTreeMap<LogicalDataSourceName, TenantDataBindingDocument>);

/// The error returned when [`TenantDataBindings::try_new`] is given no
/// bindings at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a tenant binding's data map must not be empty")]
pub struct EmptyTenantDataBindingsError;

impl TenantDataBindings {
    /// Builds a non-empty set of a tenant's data source bindings.
    ///
    /// # Errors
    ///
    /// Returns [`EmptyTenantDataBindingsError`] if `bindings` has no entries.
    pub fn try_new(
        bindings: BTreeMap<LogicalDataSourceName, TenantDataBindingDocument>,
    ) -> Result<Self, EmptyTenantDataBindingsError> {
        if bindings.is_empty() {
            Err(EmptyTenantDataBindingsError)
        } else {
            Ok(Self(bindings))
        }
    }

    /// The empty value `#[serde(default)]` falls back to when a document
    /// omits `data` entirely. Not part of this crate's public constructor
    /// surface — see this type's own rustdoc for why.
    pub(crate) fn empty_for_deserialisation() -> Self {
        Self(BTreeMap::new())
    }

    /// Whether there are no bindings. Only ever true for a value that arrived
    /// through deserialisation, never through [`Self::try_new`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many logical bindings there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Every logical binding's data source, ignoring which logical name it is
    /// bound under.
    pub fn values(&self) -> btree_map::Values<'_, LogicalDataSourceName, TenantDataBindingDocument> {
        self.0.values()
    }

    /// Every logical binding, keyed by its logical name. The same iterator
    /// [`IntoIterator for &TenantDataBindings`](#impl-IntoIterator-for-%26TenantDataBindings)
    /// produces; this method exists so a caller can iterate without an
    /// explicit `&`.
    pub fn iter(&self) -> btree_map::Iter<'_, LogicalDataSourceName, TenantDataBindingDocument> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a TenantDataBindings {
    type Item = (&'a LogicalDataSourceName, &'a TenantDataBindingDocument);
    type IntoIter = btree_map::Iter<'a, LogicalDataSourceName, TenantDataBindingDocument>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IsolationModelDocument;

    #[test]
    fn an_empty_map_is_refused_at_construction() {
        assert!(TenantDataBindings::try_new(BTreeMap::new()).is_err());
    }

    #[test]
    fn a_non_empty_map_is_accepted() {
        let mut bindings = BTreeMap::new();
        bindings.insert(
            LogicalDataSourceName::try_new("primary").unwrap(),
            TenantDataBindingDocument {
                data_source: fabric_core::DataSourceId::try_new("sql-01").unwrap(),
                isolation: IsolationModelDocument::Database {},
            },
        );

        let bindings = TenantDataBindings::try_new(bindings).unwrap();

        assert_eq!(bindings.len(), 1);
        assert!(!bindings.is_empty());
    }

    #[test]
    fn an_absent_data_field_deserialises_to_empty_without_error() {
        let bindings: TenantDataBindings = serde_json::from_str("{}").unwrap();

        assert!(bindings.is_empty());
    }
}
