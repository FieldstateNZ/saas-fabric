//! A resource's collection, as an object in the authorization model.

use std::fmt;

use crate::naming::MAX_LENGTH;
use crate::{IdentifierError, LogicalResourceName};

/// What a collection's type name adds to its resource's name.
const TYPE_SUFFIX: &str = "_collection";

/// The collection of one resource: `customers_collection:customers`.
///
/// `list` and `create` name no record, so they are asked about this object
/// instead (ADR 0027 §2a). It is a distinct type in the model, not a record of
/// the resource's own type, so no record key is invented for it and a
/// record-level allow can never answer for it.
///
/// # Why the spelling is here
///
/// The control plane writes this type into a tenant's authorization model and
/// the runtime plane asks a Check about it. The two planes may not depend on
/// each other, so the spelling lives in the crate both share, beside
/// [`OperationKind::computed_relation`](crate::OperationKind::computed_relation).
///
/// # Why it is a type per resource
///
/// Each resource declares its own relations and what each permits
/// (ADR 0013). One shared `collection` type would have to union every
/// resource's relations into one `can_list`, which over-grants wherever two
/// resources disagree about what a relation permits.
///
/// # Bound to a tenant by the store, not by the id
///
/// The object names only the resource. The authorization front selects the
/// store and the model from the verified caller token (ADR 0016), so one
/// tenant's `customers_collection:customers` is a different object from
/// another's, and nothing a caller sends can name another tenant's.
///
/// # Its type must still be a resource name
///
/// The front parses an object's type half as a [`LogicalResourceName`], so
/// the collection type has to be one too. Two consequences, both refused by
/// [`try_new`](Self::try_new) or by a client's authorization validation
/// rather than by the front at request time:
///
/// - a resource name longer than [`MAX_RESOURCE_LENGTH`](Self::MAX_RESOURCE_LENGTH)
///   bytes has no collection type that fits;
/// - a declared resource literally named `customers_collection` would be the
///   same type as `customers`' collection
///   ([`collides_with`](Self::collides_with)).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CollectionObject {
    /// The resource whose collection this is.
    resource: LogicalResourceName,
}

impl CollectionObject {
    /// The label used in error messages.
    const KIND: &'static str = "resource name with a collection";

    /// The longest resource name whose collection type is still a valid
    /// resource name.
    pub const MAX_RESOURCE_LENGTH: usize = MAX_LENGTH - TYPE_SUFFIX.len();

    /// The collection of `resource`.
    ///
    /// # Errors
    ///
    /// [`IdentifierError::TooLong`] when the resource name is longer than
    /// [`MAX_RESOURCE_LENGTH`](Self::MAX_RESOURCE_LENGTH).
    pub fn try_new(resource: LogicalResourceName) -> Result<Self, IdentifierError> {
        let actual = resource.as_str().len();

        if actual > Self::MAX_RESOURCE_LENGTH {
            return Err(IdentifierError::TooLong {
                kind: Self::KIND,
                max: Self::MAX_RESOURCE_LENGTH,
                actual,
            });
        }

        Ok(Self { resource })
    }

    /// The resource.
    #[must_use]
    pub const fn resource(&self) -> &LogicalResourceName {
        &self.resource
    }

    /// The collection's type in the authorization model:
    /// `customers_collection`.
    #[must_use]
    pub fn type_name(&self) -> String {
        format!("{}{TYPE_SUFFIX}", self.resource)
    }

    /// Whether this collection's type is spelled the same as `resource`'s
    /// record type, so the two could not both be in one model.
    #[must_use]
    pub fn collides_with(&self, resource: &LogicalResourceName) -> bool {
        self.type_name() == resource.as_str()
    }
}

impl fmt::Display for CollectionObject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.type_name(), self.resource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &str) -> LogicalResourceName {
        LogicalResourceName::try_new(value).unwrap()
    }

    #[test]
    fn a_collection_is_its_own_type_named_for_the_resource() {
        let collection = CollectionObject::try_new(name("customers")).unwrap();

        assert_eq!(collection.type_name(), "customers_collection");
        assert_eq!(collection.to_string(), "customers_collection:customers");
    }

    #[test]
    fn the_collection_type_is_itself_a_valid_resource_name() {
        let longest = name(&"r".repeat(CollectionObject::MAX_RESOURCE_LENGTH));
        let collection = CollectionObject::try_new(longest).unwrap();

        assert!(LogicalResourceName::try_new(collection.type_name()).is_ok());
    }

    #[test]
    fn a_resource_too_long_for_a_collection_type_is_refused() {
        let too_long = name(&"r".repeat(CollectionObject::MAX_RESOURCE_LENGTH + 1));

        assert!(matches!(
            CollectionObject::try_new(too_long),
            Err(IdentifierError::TooLong {
                max: 52,
                actual: 53,
                ..
            })
        ));
    }

    #[test]
    fn a_resource_spelled_like_another_resources_collection_collides() {
        let collection = CollectionObject::try_new(name("customers")).unwrap();

        assert!(collection.collides_with(&name("customers_collection")));
        assert!(!collection.collides_with(&name("customers")));
        assert!(!collection.collides_with(&name("orders_collection")));
    }
}
