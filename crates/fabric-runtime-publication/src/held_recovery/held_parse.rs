//! Reading held bytes by the rules a publication reads them by, so a
//! diagnosis or a restore never disagrees with the plan about what blocks.

use crate::{
    DataSourceDocument, DocumentKind, DocumentManifest, ObservedDocument, PublicationError,
    TenantBindingDocument, CONTRACT_VERSION,
};

/// A JSON error as its category and position only. serde's own message
/// can quote the offending value, which is held content: an unknown enum
/// variant, a string of the wrong type. None of it may reach a report.
pub(super) fn sanitized(error: &serde_json::Error) -> String {
    let category = match error.classify() {
        serde_json::error::Category::Io => "unreadable",
        serde_json::error::Category::Syntax => "not valid JSON",
        serde_json::error::Category::Data => "not the expected shape",
        serde_json::error::Category::Eof => "truncated",
    };
    format!("{category} at line {} column {}", error.line(), error.column())
}

/// A held record's refusal, without quoting the record.
pub(super) fn record_refusal(error: &PublicationError) -> String {
    match error {
        PublicationError::Unreadable { cause, .. } => cause
            .downcast_ref::<serde_json::Error>()
            .map_or_else(|| "unreadable".to_owned(), sanitized),
        refusal @ PublicationError::BelowRollbackFloor { .. } => refusal.to_string(),
        _ => "unusable".to_owned(),
    }
}

/// A held manifest, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ManifestReading {
    /// None is held.
    Missing,
    /// Not a JSON object at all: torn or overwritten, and safe to rebuild.
    Malformed(String),
    /// Readable metadata a publication refuses and this publisher may not
    /// replace: another document's kind, or a shape it does not know.
    Conflicting(String),
    /// This document's manifest, stamped by a newer publisher contract: a
    /// publication reads it, and a restore may not replace it.
    Newer(DocumentManifest),
    /// This document's manifest, at a contract this publisher supports.
    Valid(DocumentManifest),
}

/// Reads a held manifest. Only bytes that are not a JSON object count as
/// malformed; an object that is not this document's supported manifest is
/// readable metadata in conflict, which nothing here overwrites.
pub(super) fn read_manifest(bytes: Option<&[u8]>, document: DocumentKind) -> ManifestReading {
    let Some(bytes) = bytes else {
        return ManifestReading::Missing;
    };
    match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(serde_json::Value::Object(_)) => {}
        Ok(_) => return ManifestReading::Malformed("not a JSON object".to_owned()),
        Err(error) => return ManifestReading::Malformed(sanitized(&error)),
    }
    let manifest = match serde_json::from_slice::<DocumentManifest>(bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            return ManifestReading::Conflicting(format!("unsupported manifest: {}", sanitized(&error)))
        }
    };
    if manifest.document() != document {
        return ManifestReading::Conflicting(format!(
            "the manifest describes {:?}, not {document:?}",
            manifest.document()
        ));
    }
    if manifest.contract_version() > CONTRACT_VERSION {
        return ManifestReading::Newer(manifest);
    }
    ManifestReading::Valid(manifest)
}

/// A held tenants or data-sources payload, parsed as the plan parses it.
pub(super) enum Parsed {
    Tenants(Vec<TenantBindingDocument>),
    DataSources(Vec<DataSourceDocument>),
    Catalog,
}

/// Parses a payload as the document it is held as. The catalogue is never
/// parsed before a publication, so it always passes here.
pub(super) fn parse_payload(bytes: &[u8], document: DocumentKind) -> Result<Parsed, String> {
    let parsed = match document {
        DocumentKind::Tenants => serde_json::from_slice(bytes).map(Parsed::Tenants),
        DocumentKind::DataSources => serde_json::from_slice(bytes).map(Parsed::DataSources),
        DocumentKind::Catalog => Ok(Parsed::Catalog),
    };
    parsed.map_err(|error| sanitized(&error))
}

/// Why this held document refuses every publication, if it does: a manifest
/// the adapter cannot read as this document's, a manifest whose payload is
/// gone, or a payload that does not parse.
pub(super) fn blocking(document: DocumentKind, held: &ObservedDocument) -> Option<String> {
    let manifest = read_manifest(held.manifest.as_deref(), document);
    match &manifest {
        ManifestReading::Malformed(detail) => return Some(format!("unreadable manifest: {detail}")),
        ManifestReading::Conflicting(detail) => return Some(format!("conflicting manifest: {detail}")),
        ManifestReading::Missing | ManifestReading::Valid(_) | ManifestReading::Newer(_) => {}
    }
    match held.payload.as_deref() {
        None if manifest != ManifestReading::Missing && document != DocumentKind::Catalog => {
            Some("held payload lost: the manifest is held but the payload is gone".to_owned())
        }
        None => None,
        Some(bytes) => parse_payload(bytes, document)
            .err()
            .map(|detail| format!("unreadable payload: {detail}")),
    }
}
