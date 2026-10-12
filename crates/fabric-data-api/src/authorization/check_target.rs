//! What a relation Check is about: one record, or a resource's collection.

use fabric_core::LogicalResourceName;

use crate::{CheckObject, OperationKind};

/// The thing a layer-2 question is asked about.
///
/// Two kinds, kept apart in the type so neither can stand in for the other
/// (ADR 0027 §2a):
///
/// - a **record**, `customers:42`, for `read`, `update` and `delete`;
/// - a resource's **collection**, for `list` and `create`, which name no
///   record.
///
/// A collection is never spelled as a record. There is no invented key that
/// could collide with a real one, and a record-level allow is never borrowed
/// to permit a collection operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckTarget {
    /// One record of a resource.
    Record(CheckObject),

    /// The collection of a resource.
    Collection(CollectionObject),
}

impl CheckTarget {
    /// Whether `operation` is asked about a collection rather than a record.
    #[must_use]
    pub const fn is_collection_operation(operation: OperationKind) -> bool {
        matches!(operation, OperationKind::List | OperationKind::Create)
    }
}

/// A resource's collection, as the subject of a collection-level Check.
///
/// It holds only the resource. It is bound to a tenant the same way every
/// Check is: the front selects the store and the model from the verified
/// caller token (ADR 0016), so one tenant's `customers` collection is a
/// different object from another tenant's. Nothing here, and nothing a caller
/// sends, can name another tenant's collection.
///
/// **No model defines this object yet**, so no Check is asked about one.
/// `DataAuthorization` refuses every collection operation on a checked
/// resource, and `fabric-fga-check` has no wire spelling for one. Defining the
/// object type and its `can_list` and `can_create` relations is the model
/// contract that ADR 0027 owes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionObject {
    /// The resource whose collection this is.
    resource: LogicalResourceName,
}

impl CollectionObject {
    /// The collection of `resource`.
    #[must_use]
    pub const fn new(resource: LogicalResourceName) -> Self {
        Self { resource }
    }

    /// The resource.
    #[must_use]
    pub const fn resource(&self) -> &LogicalResourceName {
        &self.resource
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_and_create_are_collection_operations_and_nothing_else_is() {
        for operation in OperationKind::ALL {
            let expected = matches!(operation, OperationKind::List | OperationKind::Create);

            assert_eq!(
                CheckTarget::is_collection_operation(operation),
                expected,
                "{operation}"
            );
        }
    }

    #[test]
    fn a_collection_is_never_equal_to_a_record_of_the_same_resource() {
        let customers = LogicalResourceName::try_new("customers").unwrap();
        let collection = CheckTarget::Collection(CollectionObject::new(customers.clone()));
        let record = CheckTarget::Record(CheckObject::try_new(customers, "collection").unwrap());

        assert_ne!(collection, record);
    }
}
