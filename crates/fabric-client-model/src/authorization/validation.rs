//! The rules an authorization configuration must satisfy before it is written.
//!
//! Kept apart from the type's own file for the reason the identity rules are:
//! these are a different concern from the shape being checked, and every one
//! of them is a rule an operator can break, so each deserves its own
//! explanation.

use crate::{AuthorizationConfiguration, DesiredStateError, ResourceAuthorization};

impl AuthorizationConfiguration {
    /// Checks everything the type system cannot.
    ///
    /// Run at the same three points identity validation is — on parse, on
    /// submission, and again after a change is merged into the document — so
    /// that "the repository never holds a document this model would refuse to
    /// read" stays true by construction.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError`] naming the first rule broken, using the
    /// field's dotted path as it appears in the document.
    pub fn validate(&self) -> Result<(), DesiredStateError> {
        self.check_resources_are_unique()?;

        for resource in &self.resources {
            resource.check()?;
        }

        Ok(())
    }

    /// A resource may be declared once.
    ///
    /// Two entries for the same resource are not additive — the second would
    /// silently win or silently lose depending on which end of the list a
    /// reader started from — so the document is refused instead.
    fn check_resources_are_unique(&self) -> Result<(), DesiredStateError> {
        for (index, resource) in self.resources.iter().enumerate() {
            let duplicated = self
                .resources
                .iter()
                .take(index)
                .any(|earlier| earlier.resource == resource.resource);

            if duplicated {
                return Err(DesiredStateError::Duplicate {
                    field: "spec.authorization.resources",
                    value: resource.resource.to_string(),
                });
            }
        }

        Ok(())
    }
}

impl ResourceAuthorization {
    /// The rules one resource's relations must satisfy.
    pub(super) fn check(&self) -> Result<(), DesiredStateError> {
        if self.relations.is_empty() {
            return Err(DesiredStateError::InvalidField {
                field: "spec.authorization.resources[].relations",
                detail: format!(
                    "{} declares no relations; a resource nobody can be related to \
                     is unreachable, so it is refused rather than reconciled",
                    self.resource
                ),
            });
        }

        for (index, relation) in self.relations.iter().enumerate() {
            let duplicated = self
                .relations
                .iter()
                .take(index)
                .any(|earlier| earlier.name == relation.name);

            if duplicated {
                return Err(DesiredStateError::Duplicate {
                    field: "spec.authorization.resources[].relations",
                    value: relation.name.to_string(),
                });
            }

            relation.check(&self.resource.to_string())?;
        }

        Ok(())
    }
}

impl crate::Relation {
    /// A relation must permit something, and must not say so twice.
    fn check(&self, resource: &str) -> Result<(), DesiredStateError> {
        if self.permits.is_empty() {
            return Err(DesiredStateError::InvalidField {
                field: "spec.authorization.resources[].relations[].permits",
                detail: format!(
                    "{}/{} permits no operation; a relation that grants nothing is \
                     more likely a half-finished edit than an intention",
                    resource, self.name
                ),
            });
        }

        for (index, operation) in self.permits.iter().enumerate() {
            if self
                .permits
                .iter()
                .take(index)
                .any(|earlier| earlier == operation)
            {
                return Err(DesiredStateError::Duplicate {
                    field: "spec.authorization.resources[].relations[].permits",
                    value: operation.to_string(),
                });
            }
        }

        Ok(())
    }
}
