//! Whether an environment's held placements are still something to trust.

#[cfg(test)]
#[path = "held_tests.rs"]
mod held_tests;
mod rules;

use std::collections::{BTreeMap, BTreeSet};

use fabric_core::TenantId;
use fabric_runtime_publication::IsolationModelDocument;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::data_sources::DataSourceDeclaration;
use crate::placements::held::rules::isolation_matches_class;
use crate::placements::record::PlacementRecord;
use crate::PlatformError;

/// Refuses a held placements document a hand edit has made incoherent.
///
/// [`data_sources::held::check_held`](crate::data_sources)'s sibling, for
/// the same reason: break-glass edits to `placements.yaml` keep working by
/// design, so a document this crate did not write can reach here, and
/// `select` is only pure if what it is handed is coherent -- a document
/// with two *different* tenants recorded at one discriminator value would
/// make "the value is unique" false out from under it.
///
/// # What this does not check
///
/// A held discriminator value is never compared against the tenant it
/// belongs to. `select` always allocates the tenant's own id (ADR 0023
/// part 2), but the record is the fact once it is written, and a
/// break-glass edit that gave a tenant an opaque value instead is honoured
/// as written, not corrected -- the same acceptance ADR 0023's
/// "Bad, and accepted" states for exactly this case. Uniqueness among
/// *different* tenants is still enforced; what a tenant's own value looks
/// like is not this function's business.
///
/// # Errors
///
/// [`PlatformError::InvalidHeldPlacements`], naming the tenant and, for a
/// broken entry, the rule it breaks -- never a path.
pub(crate) fn check_held_placements(
    placements: &[PlacementRecord],
    declared: &[DataSourceDeclaration],
) -> Result<(), PlatformError> {
    let mut seen_pairs = BTreeSet::new();
    // (data source, discriminator value) -> the tenant that already holds
    // it. Keyed by value rather than by tenant, because the collision this
    // guards is two *different* tenants landing on one value -- the same
    // tenant recording that value twice, once per logical data source it
    // placed on this source, is not a collision at all (see B2/ADR 0023
    // part 2: the value is the tenant id, so a tenant's second logical
    // placement on the same shared source always repeats its own value).
    let mut discriminator_owners: BTreeMap<(&str, &str), &TenantId> = BTreeMap::new();
    // Non-shared data sources already claimed by a whole-database
    // placement. Capped at one *regardless of tenant* -- a dedicated
    // source is one tenant's, and a second entry naming it, from any
    // tenant, is not a fact a database can be.
    let mut exclusive_holders: BTreeSet<&str> = BTreeSet::new();

    for placement in placements {
        if !seen_pairs.insert((&placement.tenant, &placement.logical)) {
            return Err(PlatformError::InvalidHeldPlacements {
                detail: format!(
                    "{} is placed more than once for {}",
                    placement.tenant, placement.logical
                ),
            });
        }

        // A break-glass entry could carry `placed_at: ""` or any other
        // non-date text; `select` never produces one, but nothing else
        // checks this on the way in, and a placements document renders
        // this value straight into the API and the console.
        if OffsetDateTime::parse(&placement.placed_at, &Rfc3339).is_err() {
            return Err(PlatformError::InvalidHeldPlacements {
                detail: format!(
                    "{} was placed at a timestamp that is not RFC 3339",
                    placement.tenant
                ),
            });
        }

        let Some(source) = declared
            .iter()
            .find(|candidate| candidate.id == placement.data_source)
        else {
            return Err(PlatformError::InvalidHeldPlacements {
                detail: format!("{} names a data source nothing declares", placement.tenant),
            });
        };

        if !isolation_matches_class(source.placement, &placement.isolation) {
            return Err(PlatformError::InvalidHeldPlacements {
                detail: format!(
                    "{} is isolated in a way {} does not serve",
                    placement.tenant, placement.data_source
                ),
            });
        }

        match &placement.isolation {
            IsolationModelDocument::Discriminator { column, value } => {
                let declared_column = source
                    .discriminator
                    .as_ref()
                    .map(|discriminator| discriminator.column.as_str());

                if declared_column != Some(column.as_str()) {
                    return Err(PlatformError::InvalidHeldPlacements {
                        detail: format!(
                            "{} is isolated by column '{column}', which {} does not declare",
                            placement.tenant, placement.data_source
                        ),
                    });
                }

                let key = (placement.data_source.as_str(), value.as_str());
                match discriminator_owners.get(&key) {
                    Some(owner) if *owner != &placement.tenant => {
                        return Err(PlatformError::InvalidHeldPlacements {
                            detail: format!(
                                "{} has two tenants recorded with discriminator value '{value}'",
                                placement.data_source
                            ),
                        });
                    }
                    _ => {
                        discriminator_owners.insert(key, &placement.tenant);
                    }
                }
            }
            IsolationModelDocument::Database {} => {
                if !exclusive_holders.insert(placement.data_source.as_str()) {
                    return Err(PlatformError::InvalidHeldPlacements {
                        detail: format!("{} already has a tenant placed on it", placement.data_source),
                    });
                }
            }
            // Refused above by `isolation_matches_class`: no placement
            // class this platform declares admits schema isolation.
            IsolationModelDocument::Schema { .. } => {}
        }
    }

    Ok(())
}
