//! Every logical resource the platform exposes.

use std::collections::BTreeMap;

use fabric_core::LogicalResourceName;

use crate::{DataApiError, ResourceDefinition};

/// The logical resources applications may address.
///
/// Platform-level and identical for every tenant. Tenants differ in *where*
/// `customers` lives, never in whether it exists — that difference is the
/// runtime binding's job, and keeping the catalogue tenant-independent is what
/// makes the application contract the same everywhere (§2).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(transparent)]
pub struct ResourceCatalog {
    resources: BTreeMap<LogicalResourceName, ResourceDefinition>,
}

impl ResourceCatalog {
    /// Builds a catalogue.
    #[must_use]
    pub fn new(resources: BTreeMap<LogicalResourceName, ResourceDefinition>) -> Self {
        Self { resources }
    }

    /// Looks up a resource.
    ///
    /// # Errors
    ///
    /// [`DataApiError::UnknownResource`] if nothing is catalogued under the
    /// name. This is a 404 about the *API surface*, not about data: it says the
    /// platform exposes no such resource, which is true regardless of tenant
    /// and therefore leaks nothing.
    pub fn resolve(&self, name: &LogicalResourceName) -> Result<&ResourceDefinition, DataApiError> {
        self.resources
            .get(name)
            .ok_or_else(|| DataApiError::UnknownResource(name.clone()))
    }

    /// The catalogued resource names.
    pub fn names(&self) -> impl Iterator<Item = &LogicalResourceName> {
        self.resources.keys()
    }

    /// How many resources are catalogued.
    #[must_use]
    pub fn len(&self) -> usize {
        self.resources.len()
    }

    /// Whether the catalogue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> ResourceCatalog {
        serde_json::from_str(
            r#"{
                "customers": {"data_source": "primary", "collection": "customers"},
                "auditEvents": {"data_source": "audit", "collection": "audit_events"}
            }"#,
        )
        .unwrap()
    }

    fn name(value: &str) -> LogicalResourceName {
        LogicalResourceName::try_new(value).unwrap()
    }

    #[test]
    fn resolves_a_catalogued_resource_to_its_logical_data_source() {
        let resource = catalog().resolve(&name("customers")).unwrap().clone();

        assert_eq!(resource.data_source.as_str(), "primary");
        assert_eq!(resource.collection.as_str(), "customers");
    }

    #[test]
    fn resources_may_use_different_logical_data_sources() {
        let catalog = catalog();

        assert_eq!(
            catalog.resolve(&name("customers")).unwrap().data_source.as_str(),
            "primary"
        );
        assert_eq!(
            catalog
                .resolve(&name("auditEvents"))
                .unwrap()
                .data_source
                .as_str(),
            "audit"
        );
    }

    #[test]
    fn a_resource_name_may_differ_from_its_collection_name() {
        // Renaming a table must not change the API.
        assert_eq!(
            catalog()
                .resolve(&name("auditEvents"))
                .unwrap()
                .collection
                .as_str(),
            "audit_events"
        );
    }

    #[test]
    fn an_uncatalogued_resource_is_rejected() {
        assert!(matches!(
            catalog().resolve(&name("invoices")).unwrap_err(),
            DataApiError::UnknownResource(_)
        ));
    }
}
