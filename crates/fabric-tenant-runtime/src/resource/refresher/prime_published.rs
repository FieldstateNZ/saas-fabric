//! Priming both registries from the published documents, and publishing
//! what they hold to resolution in one swap.

use fabric_core::{ReaderGenerationReport, RecordDigest, ServedOrigin};

use super::ResourceRefresher;
use crate::publication::{GenerationOrigin, PublishedState, PublishedStateSource, ReaderStatus, Serving};
use crate::resolution::PublishedGeneration;
use crate::resource::{ApplyReport, RegistryResource, ResourceRegistry};
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
            record_sha256,
            origin: read_from,
            data_sources: published_data_sources,
            tenants: published_tenants,
        } => {
            let applied_data_sources = prime_with(data_sources, &origin, Ok(published_data_sources))?;
            let applied_tenants = prime_with(tenants, &origin, Ok(published_tenants))?;
            let applied = [Some(applied_data_sources), Some(applied_tenants)];
            generation.publish(
                tenants,
                data_sources,
                served(number, record_sha256, &read_from, applied),
            );
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
            generation.publish(tenants, data_sources, None);
            status.applied(Serving::Unrecorded);
            primed_data_sources.and(primed_tenants).map(|_| catalog)
        }
    }
}

fn prime_with<T: RegistryResource>(
    registry: &ResourceRegistry<T>,
    origin: &str,
    loaded: Result<Vec<T>, SourceError>,
) -> Result<ApplyReport, SourceError> {
    let report = registry
        .apply_all(loaded?)
        .map_err(|refused| SourceError::NothingUsable {
            origin: origin.to_owned(),
            count: refused.published,
            reason: refused.reason,
        })?;
    logging::primed::<T>(origin, registry.len());
    Ok(report)
}

/// The generation both registries now hold exactly, or `None` when either
/// registry did not take the whole of its document as offered. Such a view
/// serves some held copies beside the generation's resources: it is not that
/// generation, and a replica must not say it is.
pub(super) fn served(
    generation: u64,
    record_sha256: RecordDigest,
    origin: &GenerationOrigin,
    applied: [Option<ApplyReport>; 2],
) -> Option<ReaderGenerationReport> {
    let whole = applied
        .iter()
        .all(|report| report.is_some_and(|report| report.took_every_resource()));
    let origin = match origin {
        GenerationOrigin::Published => ServedOrigin::Published,
        GenerationOrigin::LastKnownGood { .. } => ServedOrigin::LastKnownGood,
    };
    whole.then(|| ReaderGenerationReport::new(generation, record_sha256, origin))
}
