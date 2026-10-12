//! Priming both registries from the published documents, and publishing
//! what they hold to resolution in one swap.

use super::ResourceRefresher;
use crate::publication::{PublishedState, PublishedStateSource, ReaderStatus, Serving};
use crate::resolution::PublishedGeneration;
use crate::resource::{RegistryResource, ResourceRegistry};
use crate::{logging, DataSource, SourceError, TenantRuntimeBinding};

impl ResourceRefresher {
    /// Loads the published documents once, writes both registries, and
    /// publishes what they hold to `generation` in one swap: a complete
    /// generation is applied whole; an unrecorded set is applied a document
    /// at a time, as before the record existed.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if no generation could be applied, or if a document
    /// read cleanly but nothing in it could be served. On success, the
    /// catalogue bytes [`PublishedStateSource::load_at_start`] read with the
    /// generation applied, if the source reads one. Of a complete
    /// generation, the tenants are never applied once its data sources were
    /// refused: that would prime one registry from a generation the other
    /// does not hold.
    pub(crate) async fn prime_published(
        generation: &PublishedGeneration,
        data_sources: &ResourceRegistry<DataSource>,
        tenants: &ResourceRegistry<TenantRuntimeBinding>,
        source: &dyn PublishedStateSource,
        status: &ReaderStatus,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        let primed = prime(generation, data_sources, tenants, source, status).await;
        if let Err(error) = &primed {
            status.held(error);
        }
        primed
    }
}

async fn prime(
    generation: &PublishedGeneration,
    data_sources: &ResourceRegistry<DataSource>,
    tenants: &ResourceRegistry<TenantRuntimeBinding>,
    source: &dyn PublishedStateSource,
    status: &ReaderStatus,
) -> Result<Option<Vec<u8>>, SourceError> {
    let origin = source.describe();
    let (state, catalog) = source.load_at_start().await?;
    match state {
        PublishedState::Generation {
            generation: number,
            origin: read_from,
            data_sources: published_data_sources,
            tenants: published_tenants,
        } => {
            prime_with(data_sources, &origin, Ok(published_data_sources))?;
            prime_with(tenants, &origin, Ok(published_tenants))?;
            generation.publish(tenants, data_sources);
            logging::generation_applied(&origin, number);
            status.applied(Serving::Generation {
                generation: number,
                origin: read_from,
            });
            Ok(catalog)
        }
        PublishedState::Unrecorded {
            data_sources: published_data_sources,
            tenants: published_tenants,
        } => {
            let primed_data_sources = prime_with(data_sources, &origin, published_data_sources);
            let primed_tenants = prime_with(tenants, &origin, published_tenants);
            generation.publish(tenants, data_sources);
            status.applied(Serving::Unrecorded);
            primed_data_sources.and(primed_tenants).map(|()| catalog)
        }
    }
}

fn prime_with<T: RegistryResource>(
    registry: &ResourceRegistry<T>,
    origin: &str,
    loaded: Result<Vec<T>, SourceError>,
) -> Result<(), SourceError> {
    registry
        .apply_all(loaded?)
        .map_err(|refused| SourceError::NothingUsable {
            origin: origin.to_owned(),
            count: refused.published,
            reason: refused.reason,
        })?;
    logging::primed::<T>(origin, registry.len());
    Ok(())
}
