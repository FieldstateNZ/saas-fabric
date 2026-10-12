//! Why held evidence is not clear, part by part.

use crate::DocumentKind;

/// A held part, as a problem names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    /// The observation as a whole.
    Observation,
    /// `publication.json`.
    Record,
    /// `tenants.json` and its manifest.
    Tenants,
    /// `data-sources.json` and its manifest.
    DataSources,
    /// `catalog.json` and its manifest.
    Catalog,
    /// `issuer-registry.json`.
    IssuerRegistry,
}

impl From<DocumentKind> for Part {
    fn from(document: DocumentKind) -> Self {
        match document {
            DocumentKind::Tenants => Self::Tenants,
            DocumentKind::DataSources => Self::DataSources,
            DocumentKind::Catalog => Self::Catalog,
        }
    }
}

/// Why the held evidence is not clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    /// The parts were not read as one state: the record moved while read.
    Unstable,
    /// Every publication is refused while this holds.
    BlocksPublication,
    /// A part is not the revision or bytes the record names.
    Mismatch,
    /// A part is the recorded bytes, but they break a resource rule.
    Unusable,
}

/// One reason the held evidence is not clear.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Problem {
    /// Which part.
    pub part: Part,
    /// What kind of problem.
    pub kind: ProblemKind,
    /// What, in words. Never document content beyond identifiers.
    pub detail: String,
}
