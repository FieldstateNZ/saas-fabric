//! Every way a client's desired state can be wrong.

/// A desired-state document that cannot be accepted.
///
/// Every variant is a *caller* problem — a malformed document in Git, or an
/// identity edit that would produce one. None of them is an infrastructure
/// failure, which is why reading a repository has its own error type and this
/// one carries no I/O variant.
///
/// The messages are safe to return to an operator: they describe the document
/// the operator sent or the one already in the repository, and name no
/// credential, endpoint, or upstream system.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DesiredStateError {
    /// The document is not well-formed YAML, or does not have the expected
    /// shape.
    #[error("the client document could not be read: {detail}")]
    Malformed {
        /// What was wrong with it.
        detail: String,
    },

    /// The document declares an `apiVersion` or `kind` this model does not
    /// understand.
    ///
    /// Refused rather than ignored. A document labelled `kind: Tenant` may
    /// mean something entirely different by `spec.identity`, and guessing is
    /// how a control plane writes a valid-looking document that reconciles
    /// into the wrong resource.
    #[error("expected {expected}, found {found}")]
    UnknownDocumentKind {
        /// The `apiVersion/kind` pair this model writes.
        expected: &'static str,
        /// The pair the document actually carried.
        found: String,
    },

    /// A required field is absent.
    #[error("{field} is required")]
    MissingField {
        /// The dotted path of the field, as it appears in the document.
        field: &'static str,
    },

    /// A field is present but its value is not permitted.
    #[error("{field}: {detail}")]
    InvalidField {
        /// The dotted path of the field, as it appears in the document.
        field: &'static str,
        /// Why the value is not permitted.
        detail: String,
    },

    /// A required realm role is missing from an identity configuration.
    ///
    /// Its own variant rather than an [`Self::InvalidField`] because it is the
    /// one validation failure an operator can cause from the UI by removing a
    /// row, and it deserves a message that says which role and why it cannot
    /// go.
    #[error("{role} is a required realm role and cannot be removed")]
    RequiredRoleMissing {
        /// The role that must be present.
        role: &'static str,
    },

    /// Two entries in a list collide on the identifier that must be unique.
    #[error("{field} contains {value} more than once")]
    Duplicate {
        /// The dotted path of the list, as it appears in the document.
        field: &'static str,
        /// The value that appeared twice.
        value: String,
    },
}
