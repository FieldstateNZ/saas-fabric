//! The declarative document that carries a client's desired state.

#[cfg(test)]
mod document_tests;
mod parse;
mod render;
mod schema;

use crate::{Client, DesiredStateError, IdentityConfiguration};

pub use schema::{API_VERSION, KIND};

/// One client's stored desired state: the whole document, plus the part of it
/// this model understands.
///
/// # Why both halves are kept
///
/// The obvious design is to parse into [`Client`] and serialise back out of
/// it. That design silently deletes every section the model has no field for —
/// so an operator adding a realm role would also drop the client's feature
/// flags, and the only evidence would be in a Git diff nobody reads until
/// something stops working.
///
/// Holding `raw` as well makes the safe behaviour the default one: an edit
/// replaces exactly the sub-tree it is about, and everything else survives
/// byte for byte. The typed half is derived from `raw` and re-derived after
/// every edit, so the two cannot disagree.
///
/// # What is not preserved
///
/// Comments and blank lines. The parser is a YAML *data* parser, so a
/// round-trip through this type loses them. That is a real cost in a
/// repository humans also edit by hand, and it is why the control plane
/// rewrites only documents an operator has actually changed rather than
/// normalising the repository on read.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientDocument {
    /// The complete parsed document, including sections this model does not
    /// understand.
    raw: serde_norway::Value,

    /// The part of it this model does understand, derived from `raw`.
    client: Client,
}

impl ClientDocument {
    /// The modelled view of this document.
    #[must_use]
    pub const fn client(&self) -> &Client {
        &self.client
    }

    /// Consumes the document and returns the modelled view.
    #[must_use]
    pub fn into_client(self) -> Client {
        self.client
    }

    /// Produces a copy of this document with a different identity
    /// configuration, leaving every other section untouched.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError`] if the new configuration breaks a
    /// validation rule, or if the document has no `spec` mapping to write
    /// into.
    pub fn with_identity(&self, identity: IdentityConfiguration) -> Result<Self, DesiredStateError> {
        render::with_identity(self, identity)
    }

    /// Parses a stored document.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError`] if the text is not YAML, is not a client
    /// document, or describes a client this model would refuse to write.
    pub fn parse(text: &str) -> Result<Self, DesiredStateError> {
        parse::parse(text)
    }

    /// Renders the document back to YAML.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError::Malformed`] if the document cannot be
    /// serialised, which in practice means a value was inserted that YAML
    /// cannot represent.
    pub fn render(&self) -> Result<String, DesiredStateError> {
        render::render(&self.raw)
    }

    /// Builds a document from its two halves, once both are known good.
    pub(crate) const fn from_parts(raw: serde_norway::Value, client: Client) -> Self {
        Self { raw, client }
    }

    /// Borrows the complete parsed document.
    pub(crate) const fn raw(&self) -> &serde_norway::Value {
        &self.raw
    }
}
