//! The rules that keep a declaration translatable into an authorization
//! model (ADR 0027 §2a).
//!
//! Stated in the platform's words: a resource's collection, and the relation
//! an operation is asked as, are `fabric-core` vocabulary that both planes
//! share. What the model writer does with them is its own business; these
//! rules only make sure every document that validates has a model.

use fabric_core::authorization_names::{PRINCIPAL_TYPE, SERVICE_RESERVED};
use fabric_core::{CollectionObject, OperationKind};

use crate::{AuthorizationConfiguration, DesiredStateError, Relation, ResourceAuthorization};

impl AuthorizationConfiguration {
    /// No declared resource may be spelled like another's collection.
    ///
    /// `list` and `create` are authorized against a resource's collection, a
    /// type of its own in the model named `<resource>_collection`. A resource
    /// literally named `customers_collection`, declared beside `customers`,
    /// would be the same type, and the model could not hold both.
    pub(super) fn check_collections_are_distinct(&self) -> Result<(), DesiredStateError> {
        for declared in &self.resources {
            let Ok(collection) = CollectionObject::try_new(declared.resource.clone()) else {
                continue;
            };

            if let Some(other) = self
                .resources
                .iter()
                .find(|other| collection.collides_with(&other.resource))
            {
                return Err(DesiredStateError::InvalidField {
                    field: "spec.authorization.resources[].resource",
                    detail: format!(
                        "{} is spelled like the collection of {}, which the authorization \
                         model names {}; one of the two must be renamed",
                        other.resource,
                        declared.resource,
                        collection.type_name()
                    ),
                });
            }
        }

        Ok(())
    }
}

impl ResourceAuthorization {
    /// A declared resource is a type in the model, so it cannot take the
    /// principal type's name or one the authorization service reserves.
    ///
    /// Scoped to resources declared here. A catalogue resource with no
    /// relations is never a type in the model, and keeps every name a
    /// `LogicalResourceName` allows.
    pub(super) fn check_name_is_not_reserved(&self) -> Result<(), DesiredStateError> {
        let name = self.resource.as_str();

        if name == PRINCIPAL_TYPE || SERVICE_RESERVED.contains(&name) {
            return Err(DesiredStateError::InvalidField {
                field: "spec.authorization.resources[].resource",
                detail: format!(
                    "{name} is reserved in the authorization model ({PRINCIPAL_TYPE} is the \
                     principal type; {} are the service's own); a resource declaring \
                     relations must be named something else",
                    SERVICE_RESERVED.join(" and ")
                ),
            });
        }

        Ok(())
    }

    /// The resource's collection must have a type the model can name.
    pub(super) fn check_collection_is_nameable(&self) -> Result<(), DesiredStateError> {
        CollectionObject::try_new(self.resource.clone())
            .map(|_| ())
            .map_err(|error| DesiredStateError::InvalidField {
                field: "spec.authorization.resources[].resource",
                detail: format!(
                    "{} has no collection the authorization model can name, so nobody \
                     could ever be permitted to list or create it: {error}",
                    self.resource
                ),
            })
    }
}

impl Relation {
    /// A declared relation must not be named like the relation an operation
    /// is asked as, nor like a name the authorization service reserves.
    ///
    /// The model gives the record and collection types a computed
    /// `can_<operation>` (ADR 0013); a declared relation by the same name
    /// would be the same relation, and would be granted directly. All five are
    /// reserved on both types, whichever type defines each.
    pub(super) fn check_name_is_not_reserved(&self, resource: &str) -> Result<(), DesiredStateError> {
        let name = self.name.as_str();
        let computed = OperationKind::ALL
            .into_iter()
            .any(|operation| operation.computed_relation() == name);

        if computed || SERVICE_RESERVED.contains(&name) {
            return Err(DesiredStateError::InvalidField {
                field: "spec.authorization.resources[].relations[].name",
                detail: format!(
                    "{resource}/{name} is reserved in the authorization model (can_<operation> \
                     is what an operation is asked as; {} are the service's own); declare a \
                     noun such as viewer or editor and list what it permits",
                    SERVICE_RESERVED.join(" and ")
                ),
            });
        }

        Ok(())
    }
}
