//! The persisted last-known-good copy: one file in a writable directory.

use std::io;
use std::path::{Path, PathBuf};

use super::atomic_replace::{atomic_replace, Step};
use super::envelope::{Envelope, FORMAT};
use super::LastKnownGoodRefusal;
use crate::publication::complete_generation::CompleteGeneration;
use crate::publication::digest::sha256_hex;
use crate::publication::parsed_generation::ParsedGeneration;
use crate::publication::CatalogueCheck;
use crate::publication::READER_CONTRACT;
use crate::SourceError;

/// The file's name inside the configured directory.
pub(crate) const FILE_NAME: &str = "last-known-good.json";

/// A reader's persisted copy of the last complete generation it read from
/// the published files: the record and the tenants, data-sources and
/// catalogue bytes it names.
///
/// The directory must be writable by the reader and must not be the
/// published directory: the reader never writes where the publisher does
/// (ADR 0018's single writer). Nothing but a validated published generation
/// is ever written here, so the copy cannot hold a generation the publisher
/// did not publish.
#[derive(Debug, Clone)]
pub struct LastKnownGoodStore {
    path: PathBuf,
}

impl LastKnownGoodStore {
    /// A store in `directory`. Nothing is read or created until used.
    #[must_use]
    pub fn new(directory: impl AsRef<Path>) -> Self {
        Self {
            path: directory.as_ref().join(FILE_NAME),
        }
    }

    /// The file the copy lives in.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the copy and checks it against its own record.
    ///
    /// `Ok(None)` when there is no copy. A copy is refused if any digest,
    /// the record's own included, does not match, if its record's reader
    /// floor is above this reader, or if it could not be served whole
    /// (`ParsedGeneration::validate`, with the host's `catalogue` check).
    ///
    /// # Errors
    ///
    /// [`LastKnownGoodRefusal`] saying why the copy may not be served.
    pub(crate) async fn load(
        &self,
        catalogue: &dyn CatalogueCheck,
    ) -> Result<Option<ParsedGeneration>, LastKnownGoodRefusal> {
        let bytes = match tokio::fs::read(&self.path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(cause) => {
                return Err(LastKnownGoodRefusal::Unreadable {
                    path: self.path.display().to_string(),
                    cause,
                })
            }
        };
        verify(&bytes, catalogue).map(Some)
    }

    /// Writes `generation` over the copy, atomically. Refused unless the
    /// catalogue was read with it, since a copy without its catalogue could
    /// not start a replica.
    ///
    /// # Errors
    ///
    /// [`io::Error`] from creating the directory or any write step.
    pub(crate) async fn persist(
        &self,
        generation: &CompleteGeneration,
        checkpoint: impl Fn(Step) -> io::Result<()>,
    ) -> io::Result<()> {
        let Some(catalog) = &generation.catalog else {
            return Err(io::Error::other(
                "the catalogue was not read with this generation",
            ));
        };
        let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).map_err(io::Error::other);
        let envelope = Envelope {
            format: FORMAT,
            record_sha256: generation.record_sha256(),
            record: text(&generation.record)?,
            data_sources: text(&generation.data_sources)?,
            tenants: text(&generation.tenants)?,
            catalog: text(catalog)?,
        };
        let bytes = serde_json::to_vec(&envelope).map_err(io::Error::other)?;
        if let Some(directory) = self.path.parent() {
            tokio::fs::create_dir_all(directory).await?;
        }
        atomic_replace(&self.path, &bytes, checkpoint).await
    }
}

/// Checks a copy's bytes by the same rule the published files are checked
/// by, plus the record's own digest.
pub(super) fn verify(
    bytes: &[u8],
    catalogue: &dyn CatalogueCheck,
) -> Result<ParsedGeneration, LastKnownGoodRefusal> {
    let malformed = |detail: String| LastKnownGoodRefusal::Malformed { detail };
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|error| malformed(error.to_string()))?;
    if envelope.format != FORMAT {
        return Err(LastKnownGoodRefusal::UnsupportedFormat {
            found: envelope.format,
        });
    }
    if sha256_hex(envelope.record.as_bytes()) != envelope.record_sha256 {
        return Err(LastKnownGoodRefusal::DigestMismatch { document: "record" });
    }
    let origin = "last-known-good";
    let generation = CompleteGeneration::verify(
        envelope.record.into_bytes(),
        envelope.data_sources.into_bytes(),
        envelope.tenants.into_bytes(),
        Some(envelope.catalog.into_bytes()),
        origin,
    )
    .map_err(|error| match error {
        SourceError::IncompleteGeneration { document, .. } => {
            LastKnownGoodRefusal::DigestMismatch { document }
        }
        SourceError::UnsupportedContract { required, .. } => LastKnownGoodRefusal::UnsupportedContract {
            required,
            supported: READER_CONTRACT,
        },
        other => malformed(other.to_string()),
    })?;
    ParsedGeneration::validate(generation, catalogue, origin).map_err(|error| match error {
        SourceError::Unusable { document, reason, .. } => LastKnownGoodRefusal::Unusable { document, reason },
        other => malformed(other.to_string()),
    })
}
