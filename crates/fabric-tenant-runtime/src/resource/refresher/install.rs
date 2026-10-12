//! Installing one loaded state as the view requests read: whole, or not at
//! all.

use fabric_core::{ReaderGenerationReport, RecordDigest, ServedOrigin};

use crate::publication::{GenerationOrigin, PublishedState, Serving};
use crate::resolution::PublishedGeneration;
use crate::resource::{ApplyReport, RegistryResource, ResourceRegistry};
use crate::{DataSource, SourceError, TenantRuntimeBinding};

/// The view every request reads, and the two registries it is staged in.
pub(super) type Registries<'a> = (
    &'a PublishedGeneration,
    &'a ResourceRegistry<DataSource>,
    &'a ResourceRegistry<TenantRuntimeBinding>,
);

/// What an install put in front of requests.
pub(super) struct Installed {
    pub(super) serving: Serving,
    /// The recorded generation installed, for the source to raise its floor
    /// and persist: `None` for documents with no record.
    pub(super) recorded: Option<(u64, RecordDigest)>,
}

/// Writes both registries, then swaps them in with the catalogue as one
/// view.
///
/// A recorded generation is installed whole or not at all (G8). Both
/// registries are asked first whether they would take every resource as
/// offered, and nothing is written unless both would, so a refused candidate
/// leaves the installed view, its stamp and the registries as they were.
/// Its catalogue was already decoded by the source, against the same record.
/// Documents with no record are applied as each registry judges them, and
/// installed with their catalogue under no generation.
///
/// # Errors
///
/// [`SourceError::Unusable`] naming the document a registry would not take
/// whole, or [`SourceError::NothingUsable`] for a first load that would
/// install nothing.
pub(super) fn install(
    state: PublishedState,
    (generation, data_sources, tenants): Registries<'_>,
    origin: &str,
) -> Result<Installed, SourceError> {
    match state {
        PublishedState::Generation {
            generation: number,
            record_sha256,
            origin: read_from,
            data_sources: offered_data_sources,
            tenants: offered_tenants,
            catalogue,
        } => {
            let refused = |document: &'static str, reason: String| SourceError::Unusable {
                origin: origin.to_owned(),
                generation: number,
                document,
                reason,
            };
            if let Some(reason) = data_sources.refuses_whole(&offered_data_sources) {
                return Err(refused("data-sources", reason));
            }
            if let Some(reason) = tenants.refuses_whole(&offered_tenants) {
                return Err(refused("tenants", reason));
            }
            apply(data_sources, origin, offered_data_sources)?;
            apply(tenants, origin, offered_tenants)?;
            let served = ReaderGenerationReport::new(number, record_sha256, served_origin(&read_from));
            generation.publish(catalogue, tenants, data_sources, Some(served));
            Ok(Installed {
                serving: Serving::Generation {
                    generation: number,
                    origin: read_from,
                },
                recorded: Some((number, record_sha256)),
            })
        }
        PublishedState::Unrecorded {
            data_sources: offered_data_sources,
            tenants: offered_tenants,
            catalogue,
        } => {
            apply(data_sources, origin, offered_data_sources)?;
            apply(tenants, origin, offered_tenants)?;
            generation.publish(catalogue, tenants, data_sources, None);
            Ok(Installed {
                serving: Serving::Unrecorded,
                recorded: None,
            })
        }
    }
}

fn apply<T: RegistryResource>(
    registry: &ResourceRegistry<T>,
    origin: &str,
    resources: Vec<T>,
) -> Result<ApplyReport, SourceError> {
    registry
        .apply_all(resources)
        .map_err(|refused| SourceError::NothingUsable {
            origin: origin.to_owned(),
            count: refused.published,
            reason: refused.reason,
        })
}

const fn served_origin(origin: &GenerationOrigin) -> ServedOrigin {
    match origin {
        GenerationOrigin::Published => ServedOrigin::Published,
        GenerationOrigin::LastKnownGood { .. } => ServedOrigin::LastKnownGood,
    }
}
