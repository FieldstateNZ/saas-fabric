//! Reading the catalogue as part of the generation the publication record
//! names (G8).

use std::path::Path;

use super::digest::sha256_hex;
use super::published_files::{parse, read, read_optional};
use super::record::{ReaderRecord, PUBLICATION_RECORD_FILE, READER_CONTRACT};
use crate::SourceError;

/// The catalogue's bytes at `catalogue_path`, refused unless they are the
/// catalogue the publication record beside `tenants_path` names.
///
/// The catalogue is read once, at process start, from its own file -- in
/// Kubernetes its own volume, projected on its own. Without this a process
/// could start on a catalogue from one generation beside tenants from
/// another. With no record (a publisher older than the record) the
/// catalogue is read as it stands.
///
/// # Errors
///
/// [`SourceError::IncompleteGeneration`] naming `catalog` when the bytes
/// differ from the record's digest; [`SourceError::UnsupportedContract`]
/// for a record above this reader; [`SourceError::Unreadable`] or
/// [`SourceError::Malformed`] for a file that cannot be read or parsed.
pub async fn read_recorded_catalogue(
    tenants_path: impl AsRef<Path>,
    catalogue_path: impl AsRef<Path>,
) -> Result<Vec<u8>, SourceError> {
    let record_path = tenants_path.as_ref().with_file_name(PUBLICATION_RECORD_FILE);
    let origin = format!(
        "catalogue: {}, record={}",
        catalogue_path.as_ref().display(),
        record_path.display()
    );
    let Some(record) = read_optional(&record_path, &origin).await? else {
        return read(catalogue_path.as_ref(), &origin).await;
    };
    let record: ReaderRecord = parse(&record, &origin)?;
    if record.minimum_reader_contract > READER_CONTRACT {
        return Err(SourceError::UnsupportedContract {
            origin,
            required: record.minimum_reader_contract,
            supported: READER_CONTRACT,
        });
    }
    let catalogue = read(catalogue_path.as_ref(), &origin).await?;
    if sha256_hex(&catalogue) != record.documents.catalog.sha256 {
        return Err(SourceError::IncompleteGeneration {
            origin,
            generation: record.generation,
            document: "catalog",
        });
    }
    Ok(catalogue)
}
