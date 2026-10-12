//! Turning a choice into the state a refresher applies.

use super::PublishedFiles;
use crate::publication::complete_generation::CompleteGeneration;
use crate::publication::file_reads::read;
use crate::publication::last_known_good::Choice;
use crate::publication::parsed_generation::ParsedGeneration;
use crate::publication::{GenerationOrigin, PublishedState, PublishedStateSource as _};
use crate::{JsonFileSource, ResourceSource as _, SourceError};

impl PublishedFiles {
    /// What a choice applies, with the catalogue read alongside it. With no
    /// record, the catalogue is read as it stands, and only `at_start`.
    pub(super) async fn apply(
        &self,
        choice: Choice,
        at_start: bool,
    ) -> Result<(PublishedState, Option<Vec<u8>>), SourceError> {
        let origin = self.describe();
        let (generation, origin_of) = match choice {
            Choice::Published(generation) => (generation, GenerationOrigin::Published),
            Choice::LastKnownGood {
                generation,
                held_because,
            } => (generation, GenerationOrigin::LastKnownGood { held_because }),
            Choice::Refused(error) => return Err(error),
            Choice::Unrecorded => {
                let catalog = match self.catalog.as_deref().filter(|_| at_start) {
                    Some(path) => Some(read(path, &origin).await?),
                    None => None,
                };
                let state = PublishedState::Unrecorded {
                    data_sources: JsonFileSource::new(&self.data_sources).load().await,
                    tenants: JsonFileSource::new(&self.tenants).load().await,
                };
                return Ok((state, catalog));
            }
        };
        let state = PublishedState::Generation {
            generation: generation.generation(),
            origin: origin_of,
            data_sources: generation.data_sources,
            tenants: generation.tenants,
        };
        Ok((state, generation.complete.catalog))
    }
}

/// Without a copy: a complete generation is decoded and applied, as before
/// the copy existed (the registry judges each resource); no record is read
/// per document; anything else is refused.
pub(super) fn to_choice(read: Result<Option<CompleteGeneration>, SourceError>, origin: &str) -> Choice {
    match read.and_then(|read| {
        read.map(|generation| ParsedGeneration::parse(generation, origin))
            .transpose()
    }) {
        Ok(Some(generation)) => Choice::Published(generation),
        Ok(None) => Choice::Unrecorded,
        Err(error) => Choice::Refused(error),
    }
}
