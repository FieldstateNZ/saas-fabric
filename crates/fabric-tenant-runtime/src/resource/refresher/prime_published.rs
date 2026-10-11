//! Priming both registries from the published documents, and publishing
//! what they hold to resolution in one swap.

use super::ResourceRefresher;
use crate::publication::{PublishedState, PublishedStateSource};
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
    /// read cleanly but nothing in it could be served. Of a complete
    /// generation, the tenants are never applied once its data sources were
    /// refused: that would prime one registry from a generation the other
    /// does not hold.
    pub(crate) async fn prime_published(
        generation: &PublishedGeneration,
        data_sources: &ResourceRegistry<DataSource>,
        tenants: &ResourceRegistry<TenantRuntimeBinding>,
        source: &dyn PublishedStateSource,
    ) -> Result<(), SourceError> {
        let origin = source.describe();
        match source.load().await? {
            PublishedState::Generation {
                generation: number,
                data_sources: published_data_sources,
                tenants: published_tenants,
            } => {
                prime_with(data_sources, &origin, Ok(published_data_sources))?;
                prime_with(tenants, &origin, Ok(published_tenants))?;
                generation.publish(tenants, data_sources);
                logging::generation_applied(&origin, number);
                Ok(())
            }
            PublishedState::Unrecorded {
                data_sources: published_data_sources,
                tenants: published_tenants,
            } => {
                let primed_data_sources = prime_with(data_sources, &origin, published_data_sources);
                let primed_tenants = prime_with(tenants, &origin, published_tenants);
                generation.publish(tenants, data_sources);
                primed_data_sources.and(primed_tenants)
            }
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
