//! The records a publication pass leaves in [`super::PublicationState`].

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, TenantId};

use crate::publication::outcome::PassOutcome;

/// What one publication pass did, and when it finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastPass {
    /// When it finished, as seconds since the Unix epoch.
    ///
    /// Not formatted here, for the same reason `LastCheck::at_unix_seconds`
    /// is not: recording a fact and rendering it for a human are different
    /// jobs, and only one of them belongs in a crate with no transport.
    pub at_unix_seconds: u64,

    /// What it found and did.
    pub outcome: PassOutcome,
}

/// The tenants a held tenants document carries, and when that was known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedTenants {
    /// When the pass that established it finished, as seconds since the
    /// Unix epoch.
    pub at_unix_seconds: u64,

    /// Each published tenant, at its binding revision.
    pub tenants: BTreeMap<TenantId, BindingRevision>,
}
