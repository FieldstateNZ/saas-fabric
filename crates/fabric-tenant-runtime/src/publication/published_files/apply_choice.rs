//! Turning a choice into the state a refresher installs.

use super::PublishedFiles;
use crate::publication::complete_generation::CompleteGeneration;
use crate::publication::file_reads::read;
use crate::publication::last_known_good::{on_refresh, Choice};
use crate::publication::parsed_generation::ParsedGeneration;
use crate::publication::{CatalogueDecoder, GenerationOrigin, PublishedState, PublishedStateSource as _};
use crate::{JsonFileSource, ResourceSource as _, SourceError};

impl PublishedFiles {
    /// What a choice installs. A published generation is kept as pending
    /// until the refresher says it was installed. With no record, all three
    /// documents are read and decoded now, and any that cannot be holds the
    /// whole view.
    pub(super) async fn apply(&self, choice: Choice) -> Result<PublishedState, SourceError> {
        let (generation, origin) = match choice {
            Choice::Published(generation) => {
                self.progress().pending = Some(generation.clone());
                (generation, GenerationOrigin::Published)
            }
            Choice::LastKnownGood {
                generation,
                held_because,
            } => (generation, GenerationOrigin::LastKnownGood { held_because }),
            Choice::Refused(error) => return Err(error),
            Choice::Unrecorded => return self.unrecorded().await,
        };
        Ok(PublishedState::Generation {
            generation: generation.generation(),
            record_sha256: generation.complete.record_digest(),
            origin,
            data_sources: generation.data_sources,
            tenants: generation.tenants,
            catalogue: generation.catalogue,
        })
    }

    async fn unrecorded(&self) -> Result<PublishedState, SourceError> {
        let origin = self.describe();
        let data_sources = JsonFileSource::new(&self.data_sources).load().await?;
        let tenants = JsonFileSource::new(&self.tenants).load().await?;
        let bytes = read(&self.catalog, &origin).await?;
        let catalogue = self
            .decoder
            .decode(&bytes)
            .map_err(|detail| SourceError::Malformed {
                origin: format!("catalogue: {}", self.catalog.display()),
                detail,
            })?;
        Ok(PublishedState::Unrecorded {
            data_sources,
            tenants,
            catalogue,
        })
    }
}

/// Without a copy: a complete generation is decoded, catalogue included, and
/// offered under the same roll-forward rule a copy's holder applies, with
/// `recorded`, the highest recorded generation this process installed, as
/// the floor (G8). A lower generation, or a lost record, holds the installed
/// view as `SourceError::Regressed` rather than going back to it or to
/// documents nothing proves belong together. With nothing recorded
/// installed yet, no record is read per document. The floor is this
/// process's own and is not durable.
pub(super) fn without_copy(
    read: Result<Option<CompleteGeneration>, SourceError>,
    decoder: &dyn CatalogueDecoder,
    recorded: Option<u64>,
    origin: &str,
) -> Choice {
    let parsed = read.and_then(|read| {
        read.map(|generation| ParsedGeneration::parse(generation, decoder, origin))
            .transpose()
    });
    on_refresh(parsed, recorded, origin)
}
