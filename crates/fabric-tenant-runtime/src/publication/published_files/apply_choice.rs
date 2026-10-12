//! Turning a choice into the state a refresher installs.

use super::PublishedFiles;
use crate::publication::complete_generation::CompleteGeneration;
use crate::publication::file_reads::read;
use crate::publication::last_known_good::Choice;
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
/// offered; anything else is refused. No record is read per document only
/// while no recorded generation has been installed: once one has
/// (`recorded`), losing the record holds it rather than downgrading to
/// documents nothing proves belong together (G8).
pub(super) fn without_copy(
    read: Result<Option<CompleteGeneration>, SourceError>,
    decoder: &dyn CatalogueDecoder,
    recorded: Option<u64>,
    origin: &str,
) -> Choice {
    match read.and_then(|read| {
        read.map(|generation| ParsedGeneration::parse(generation, decoder, origin))
            .transpose()
    }) {
        Ok(Some(generation)) => Choice::Published(generation),
        Ok(None) => match recorded {
            Some(held) => Choice::Refused(SourceError::Regressed {
                origin: origin.to_owned(),
                held,
                found: "no publication record".to_owned(),
            }),
            None => Choice::Unrecorded,
        },
        Err(error) => Choice::Refused(error),
    }
}
