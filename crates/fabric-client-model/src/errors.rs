//! Every way a client's desired state can be wrong.
//!
//! In the 121–150 line band. The reason is the same one
//! `fabric-control-plane`'s own `errors.rs` gives for its own exemption
//! from the 150-line hard limit: this is one enum, every way a document can
//! be refused, with the reasoning for each variant's message beside it.
//! Splitting it would put some refusals in one file and some in another,
//! with no principled line between them — the next person adding a
//! variant would have to guess which file, rather than adding one more
//! arm to the one place they all already are.

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

    /// The catalogue document is not well-formed YAML, or does not have the
    /// expected shape.
    ///
    /// [`Self::Malformed`]'s sibling for the one document that is not a
    /// client's. Kept apart rather than reused with a generic message,
    /// because "the client document could not be read" is actively
    /// misleading on the one document in the repository that is not a
    /// client's — an operator reading it on the Applications or Settings
    /// page would go looking for the wrong file.
    #[error("the catalogue could not be read: {detail}")]
    CatalogueMalformed {
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
    #[error("expected {expected}, found {}", found.as_deref().unwrap_or("no apiVersion or kind at all"))]
    UnknownDocumentKind {
        /// The `apiVersion/kind` pair this model writes.
        expected: &'static str,
        /// The pair the document actually carried — `None` when it carried
        /// neither field at all, which is what a document written before
        /// this envelope concept existed looks like, and distinct from
        /// `Some` naming a pair this build simply does not (yet, or any
        /// longer) recognise. A caller that only wants "is this the
        /// legacy, pre-envelope shape or something else" reads this field
        /// rather than parsing [`Self`]'s rendered message.
        found: Option<String>,
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

    /// A shape this model can represent but this phase does not reconcile.
    ///
    /// Its own variant rather than an [`Self::InvalidField`] because the two
    /// say different things to an operator. "That value is not permitted"
    /// invites a different value; "that value is not carried yet, by this
    /// phase" invites a decision about when. Naming the phase is the whole
    /// point, and a shape that is representable now is one whose document does
    /// not have to change again when the phase lands.
    #[error("{field}: {detail} (deferred to {phase})")]
    Deferred {
        /// The dotted path of the field, as it appears in the document.
        field: &'static str,
        /// The phase that will carry it.
        phase: &'static str,
        /// What was declared, and what to do instead in the meantime.
        detail: String,
    },

    /// A document written before a change of shape, naming what replaced it.
    ///
    /// Separate from [`Self::Malformed`] for the reason the document-kind
    /// check is separate from deserialisation: a message about a missing field
    /// sends an operator looking for something their document was never
    /// supposed to have, where naming the replacement points them at the
    /// actual problem. Every use of it names the version and the field.
    #[error("{field} was replaced by {replacement}: {detail}")]
    Migration {
        /// The dotted path of the field the document still carries, or the one
        /// it cannot be read into.
        field: &'static str,
        /// What replaced it.
        replacement: &'static str,
        /// What the document says, and what has to happen to it.
        detail: String,
    },
}
