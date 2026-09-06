//! The declarative document that carries a client's desired state.
//!
//! Over the 120-line advisory threshold. The reason is that this is one
//! struct together with its impls: the two halves it holds, the accessors
//! that read them, and the three operations — parse, edit, render — that
//! produce and consume it. None of those would be reused or tested apart
//! from the type they belong to.

#[cfg(test)]
mod document_tests;
mod migration;
mod parse;
mod render;
mod schema;
mod version;

use crate::{Client, DesiredStateError, IdentityConfiguration};

pub use schema::{API_VERSION, API_VERSION_V2, KIND};

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
/// replaces exactly the sub-tree it is about, and every other key and value —
/// and their order — survives unchanged. The typed half is derived from `raw`
/// and re-derived after every edit, so the two cannot disagree.
///
/// # What is not preserved
///
/// **Formatting.** This is a YAML *data* parser and [`render`](Self::render)
/// reprints the whole file, so a round trip loses comments and blank lines,
/// normalises quoting, turns flow sequences into block sequences, and returns
/// a folded scalar as a literal block. The *values* are identical either way;
/// the file is not.
///
/// That is a real cost in a repository humans also edit by hand, and it is why
/// the control plane rewrites only documents an operator has actually changed
/// rather than normalising the repository on read.
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

    /// Which schema version this document declares.
    ///
    /// Read from `raw` rather than decided once at parse time and cached, so
    /// it can never drift from what the file actually says. A caller checking
    /// which schema a shipped example exercises should ask the document this
    /// rather than search the text for `apiVersion: ...` — a rename of either
    /// constant would leave a text search checking nothing.
    ///
    /// Both constants are matched by name, and anything else is `None` — which
    /// is what makes this an answer rather than a guess. It used to read
    /// "`v2` if it says `v2`, otherwise `v1`", so a document declaring
    /// something neither constant names would have been reported as `v1`.
    ///
    /// That third case cannot arise: `version::check_document_kind` runs
    /// before the document is deserialised and refuses any pair that is not
    /// one of the two, so a `ClientDocument` only ever exists for a version
    /// this model reads. `unwrap_or` supplies `v1` for it because this crate
    /// does not panic in production code and a wrong answer about a schema
    /// label is not worth a process; adding a `v3` without adding an arm here
    /// is then a visible omission rather than a silent misreport.
    #[must_use]
    pub fn api_version(&self) -> &'static str {
        let declared = self
            .raw
            .as_mapping()
            .and_then(|mapping| mapping.get("apiVersion"))
            .and_then(serde_norway::Value::as_str);

        match declared {
            Some(schema::API_VERSION_V2) => Some(schema::API_VERSION_V2),
            Some(schema::API_VERSION) => Some(schema::API_VERSION),
            _ => None,
        }
        .unwrap_or(schema::API_VERSION)
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
