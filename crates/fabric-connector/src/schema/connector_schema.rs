//! Everything a backend exposes.

use std::collections::BTreeMap;

use crate::{CollectionName, CollectionSchema, ConnectorError, FieldName};

/// The collections a connector exposes, and their fields.
///
/// Fetched once at startup and cached. It describes the *backend*, not a
/// tenant — every tenant on a connector sees the same collections, and which
/// rows they can reach is settled by the connection and the isolation model,
/// not by the schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectorSchema {
    collections: BTreeMap<CollectionName, CollectionSchema>,
}

impl ConnectorSchema {
    /// Builds a schema from its collections.
    #[must_use]
    pub fn new(collections: impl IntoIterator<Item = (CollectionName, CollectionSchema)>) -> Self {
        Self {
            collections: collections.into_iter().collect(),
        }
    }

    /// Looks up a collection.
    #[must_use]
    pub fn collection(&self, name: &CollectionName) -> Option<&CollectionSchema> {
        self.collections.get(name)
    }

    /// The collection names, in order.
    pub fn collection_names(&self) -> impl Iterator<Item = &CollectionName> {
        self.collections.keys()
    }

    /// Checks that a collection exists and has every named field.
    ///
    /// Called before an operation is sent, so that a catalogue pointing at a
    /// collection the backend does not have produces a clear platform error
    /// rather than a vendor-specific one.
    ///
    /// # Errors
    ///
    /// - [`ConnectorError::UnknownCollection`] if the collection is absent.
    /// - [`ConnectorError::InvalidOperation`] if a named field is absent.
    pub fn ensure_fields(
        &self,
        collection: &CollectionName,
        fields: &[&FieldName],
    ) -> Result<(), ConnectorError> {
        let schema = self
            .collection(collection)
            .ok_or_else(|| ConnectorError::UnknownCollection(collection.clone()))?;

        for field in fields {
            if !schema.has_field(field) {
                return Err(ConnectorError::InvalidOperation(format!(
                    "collection {collection} has no field {field}"
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> ConnectorSchema {
        ConnectorSchema::new([(
            CollectionName::try_new("customers").unwrap(),
            CollectionSchema::new([
                FieldName::try_new("id").unwrap(),
                FieldName::try_new("name").unwrap(),
            ]),
        )])
    }

    #[test]
    fn accepts_fields_the_collection_has() {
        let id = FieldName::try_new("id").unwrap();
        let collection = CollectionName::try_new("customers").unwrap();

        assert!(schema().ensure_fields(&collection, &[&id]).is_ok());
    }

    #[test]
    fn rejects_an_unknown_collection() {
        let collection = CollectionName::try_new("invoices").unwrap();

        assert!(matches!(
            schema().ensure_fields(&collection, &[]).unwrap_err(),
            ConnectorError::UnknownCollection(_)
        ));
    }

    #[test]
    fn rejects_a_field_the_collection_does_not_have() {
        let collection = CollectionName::try_new("customers").unwrap();
        let ghost = FieldName::try_new("secret_salary").unwrap();

        assert!(matches!(
            schema().ensure_fields(&collection, &[&ghost]).unwrap_err(),
            ConnectorError::InvalidOperation(_)
        ));
    }
}
