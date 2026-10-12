//! What a relation Check is about: one record, or a resource's collection.

use fabric_core::CollectionObject;

use crate::CheckObject;

/// The thing a layer-2 question is asked about.
///
/// Two kinds, kept apart in the type so neither can stand in for the other
/// (ADR 0027 §2a):
///
/// - a **record**, `customers:42`, for `read`, `update` and `delete`;
/// - a resource's **collection**, `customers_collection:customers`, for `list`
///   and `create`, which name no record.
///
/// Which operations are collection operations is
/// [`OperationKind::is_collection_operation`](crate::OperationKind::is_collection_operation),
/// the same partition the model writer uses. A collection is a distinct type
/// in the model, so no record key is invented for it and a record-level allow
/// is never borrowed to permit a collection operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckTarget {
    /// One record of a resource.
    Record(CheckObject),

    /// The collection of a resource.
    Collection(CollectionObject),
}

#[cfg(test)]
mod tests {
    use fabric_core::LogicalResourceName;

    use super::*;

    #[test]
    fn a_collection_is_never_equal_to_a_record_of_the_same_resource() {
        let customers = LogicalResourceName::try_new("customers").unwrap();
        let collection = CollectionObject::try_new(customers.clone()).unwrap();
        let record = CheckObject::try_new(customers.clone(), "customers").unwrap();
        let record_named_like_the_collection = CheckObject::try_new(customers, "collection").unwrap();

        assert_ne!(
            CheckTarget::Collection(collection.clone()),
            CheckTarget::Record(record)
        );
        assert_ne!(
            CheckTarget::Collection(collection),
            CheckTarget::Record(record_named_like_the_collection)
        );
    }

    #[test]
    fn a_collection_and_a_record_never_render_as_the_same_object() {
        let customers = LogicalResourceName::try_new("customers").unwrap();
        let collection = CollectionObject::try_new(customers.clone()).unwrap();

        for key in ["customers", "collection", "customers_collection"] {
            let record = CheckObject::try_new(customers.clone(), key).unwrap();

            assert_ne!(collection.to_string(), record.to_string(), "{key}");
        }
    }
}
