//! The catalogue a runtime starts on: the one read with the generation it
//! primed, or, with nothing primed, the one the publication record names
//! (G8).

use std::path::Path;

use super::digest::sha256_hex;
use super::file_reads::{parse, read, read_optional};
use super::record::{ReaderRecord, PUBLICATION_RECORD_FILE, READER_CONTRACT};
use crate::SourceError;

/// The catalogue a runtime should start on.
///
/// `primed` is [`PublishedRuntime::catalogue`](crate::PublishedRuntime::catalogue):
/// the bytes read in the same read as the generation the resolver primed
/// from. When present they are used as they are, and nothing is read again,
/// so a publication landing after the prime cannot pair its catalogue with
/// the previous generation's tenants.
///
/// With nothing primed (the runtime started unprimed and holds no
/// generation yet), the catalogue at `catalogue_path` is read and refused
/// unless it is the one the publication record beside `tenants_path` names;
/// with no record, it is read as it stands. The catalogue is read only at
/// start: a running replica keeps it while its tenants refresh.
///
/// # Errors
///
/// [`SourceError::IncompleteGeneration`] naming `catalog` when the bytes
/// differ from the record's digest; [`SourceError::UnsupportedContract`]
/// for a record above this reader; [`SourceError::Unreadable`] or
/// [`SourceError::Malformed`] for a file that cannot be read or parsed.
pub async fn startup_catalogue(
    primed: Option<Vec<u8>>,
    tenants_path: impl AsRef<Path>,
    catalogue_path: impl AsRef<Path>,
) -> Result<Vec<u8>, SourceError> {
    if let Some(primed) = primed {
        return Ok(primed);
    }
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
