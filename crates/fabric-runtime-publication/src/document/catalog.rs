//! Every logical resource the platform exposes, as the publisher declares
//! it.

use std::collections::BTreeMap;

use fabric_core::LogicalResourceName;

use crate::canonical::to_canonical_bytes;
use crate::ResourceDefinitionDocument;

/// The publisher's own declaration of the resource catalogue.
///
/// Mirrors `fabric_data_api::ResourceCatalog`, but that type derives
/// `Deserialize` only — nothing in the consumer's own dependency graph can
/// *write* a catalogue, which is exactly the gap this crate exists to close.
/// `#[serde(transparent)]` keeps `catalog.json` a bare JSON object, matching
/// what the runtime reads today.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CatalogDocument(BTreeMap<LogicalResourceName, ResourceDefinitionDocument>);

impl CatalogDocument {
    /// Builds a catalogue document from its resources.
    #[must_use]
    pub fn new(resources: BTreeMap<LogicalResourceName, ResourceDefinitionDocument>) -> Self {
        Self(resources)
    }

    /// How many resources are catalogued.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the catalogue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over every catalogued resource, keyed by its logical name.
    pub fn resources(&self) -> impl Iterator<Item = (&LogicalResourceName, &ResourceDefinitionDocument)> {
        self.0.iter()
    }

    /// Looks up one resource by its logical name.
    #[must_use]
    pub fn get(&self, name: &LogicalResourceName) -> Option<&ResourceDefinitionDocument> {
        self.0.get(name)
    }

    /// Renders this catalogue as canonical JSON (two-space indentation, a
    /// trailing newline, UTF-8).
    ///
    /// A `BTreeMap` already orders by key, so — unlike the array-shaped
    /// documents — no separate sort step is needed here.
    ///
    /// # Errors
    ///
    /// Returns [`serde_json::Error`] only if a resource's own `Serialize`
    /// implementation fails, which cannot happen for this crate's validated
    /// types.
    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        to_canonical_bytes(self)
    }
}

#[cfg(test)]
mod tests {
    use fabric_core::{LogicalDataSourceName, OperationKind};
    use fabric_data_api::ResourceCatalog;

    use super::*;
    use crate::{CollectionName, FieldName};

    fn catalog() -> CatalogDocument {
        let mut resources = BTreeMap::new();
        resources.insert(
            LogicalResourceName::try_new("customers").unwrap(),
            ResourceDefinitionDocument {
                data_source: LogicalDataSourceName::try_new("primary").unwrap(),
                collection: CollectionName::try_new("customers").unwrap(),
                key_field: FieldName::try_new("id").unwrap(),
                operations: vec![OperationKind::Read, OperationKind::List],
                queryable_fields: Vec::new(),
            },
        );

        CatalogDocument::new(resources)
    }

    #[test]
    fn a_published_catalogue_deserialises_as_the_runtimes_own_catalogue() {
        let bytes = catalog().canonical_json().unwrap();

        let parsed: ResourceCatalog = serde_json::from_slice(&bytes).unwrap();

        assert!(!parsed.is_empty());
        assert!(parsed
            .resolve(&LogicalResourceName::try_new("customers").unwrap())
            .is_ok());
    }

    #[test]
    fn resources_iterates_every_catalogued_entry() {
        let catalog = catalog();
        let names: Vec<&str> = catalog.resources().map(|(name, _)| name.as_str()).collect();

        assert_eq!(names, ["customers"]);
    }

    #[test]
    fn get_looks_up_one_resource_by_its_logical_name() {
        let catalog = catalog();

        assert!(catalog
            .get(&LogicalResourceName::try_new("customers").unwrap())
            .is_some());
        assert!(catalog
            .get(&LogicalResourceName::try_new("unknown").unwrap())
            .is_none());
    }
}
