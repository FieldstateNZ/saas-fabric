//! Reading the files raw for an inspection, bracketed by the record.

use std::io;
use std::path::Path;

use super::paths::DocumentPaths;
use crate::{ObservedDocument, PublicationCompleteness, TargetObservation, UnobservedReason};

/// Reads the record, then every document and the registry, then the record
/// again, and judges them. A missing file is absent; any other read failure
/// is [`UnobservedReason::Unavailable`], with its detail dropped.
pub(super) fn inspect(
    documents: [&DocumentPaths; 3],
    record: &Path,
    registry: &Path,
) -> PublicationCompleteness {
    let observed =
        observe(documents, record, registry).and_then(|observation| Ok((observation, read(record)?)));
    match observed {
        Ok((observation, after)) => observation.assess(after.as_deref()),
        Err(_) => PublicationCompleteness::Unobserved {
            reason: UnobservedReason::Unavailable,
        },
    }
}

/// Reads the record, then every document and the registry, raw. A missing
/// file is absent.
pub(super) fn observe(
    [tenants, data_sources, catalog]: [&DocumentPaths; 3],
    record: &Path,
    registry: &Path,
) -> io::Result<TargetObservation> {
    Ok(TargetObservation {
        record: read(record)?,
        tenants: document(tenants)?,
        data_sources: document(data_sources)?,
        catalog: document(catalog)?,
        registry: read(registry)?,
    })
}

fn document(paths: &DocumentPaths) -> io::Result<ObservedDocument> {
    Ok(ObservedDocument {
        manifest: read(&paths.manifest)?,
        payload: read(&paths.payload)?,
    })
}

pub(super) fn read(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
