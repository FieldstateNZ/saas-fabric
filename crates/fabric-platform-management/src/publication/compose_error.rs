//! Why [`compose`](super::compose) could not build a snapshot from what it
//! was given.

use fabric_core::{LogicalDataSourceName, TenantId};

/// Why [`compose`](super::compose) could not build a snapshot from what it
/// was given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComposeError {
    /// One tenant's logical data source is recorded twice.
    ///
    /// Held state this crate itself wrote never has this shape --
    /// `select` refuses a second placement for one (tenant, logical) -- but
    /// a break-glass edit to `placements.yaml` can produce it, and a
    /// formula that silently kept one entry over the other would make a
    /// publication disagree with whichever entry an operator reads in the
    /// file. `check_held_placements` refuses this same shape for the
    /// console; this is the same rule at the seam a scheduled pass runs
    /// through even when nothing reads the list first.
    #[error("{tenant} has two placements for {logical}")]
    DuplicatePlacement {
        /// The tenant with two records.
        tenant: TenantId,
        /// The logical data source both records name.
        logical: LogicalDataSourceName,
    },

    /// A tenant's accumulated bindings ended up empty.
    ///
    /// Structurally unreachable in this function's own code today -- a
    /// tenant only ever enters the accumulator alongside its first logical
    /// binding -- but this is library code, not a test: a defensive
    /// invariant that later becomes false (a refactor that clears `data`
    /// without also removing the tenant, say) must return a value for its
    /// caller to act on, never panic a publication pass out from under an
    /// operator.
    #[error("{tenant} has no logical data source bindings")]
    EmptyTenantBinding {
        /// The tenant whose accumulated bindings were empty.
        tenant: TenantId,
    },
}
