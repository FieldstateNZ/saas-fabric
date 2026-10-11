//! What the control plane could learn about the managed platform, read once
//! per request and shared by every client it renders.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, TenantId};
use fabric_platform_management::{LastPass, PlacementRecord, PublishedTenants};

/// The environment's placements and its publication, as far as known.
pub(crate) struct PlatformFacts {
    /// What is placed.
    pub(crate) placements: Placements,
    /// What publication has done.
    pub(crate) publication: Publication,
}

/// The environment's recorded placements.
pub(crate) enum Placements {
    /// This deployment manages no platform.
    NotManaged,
    /// They could not be read.
    Unreadable,
    /// Every recorded placement.
    Read(Vec<PlacementRecord>),
}

/// What this process knows about publication.
///
/// Publisher-side only: what the publication target accepted. Nothing here
/// says a reader applied it.
pub(crate) enum Publication {
    /// This deployment has no publication target.
    NotConfigured,
    /// It has one.
    Observed {
        /// The last completed pass, if one has run since start-up.
        last_pass: Option<LastPass>,
        /// What the last pass that published or found nothing to change
        /// left in the accepted tenants document.
        published: Option<PublishedTenants>,
    },
    /// The tenants document a publication pass is offering now, with the
    /// issuer registry decided against it: in the same generation, each
    /// tenant is published at the revision its placements add up to.
    Offered(BTreeMap<TenantId, BindingRevision>),
}

impl PlatformFacts {
    /// Facts for a deployment that manages no platform.
    pub(crate) const fn not_managed() -> Self {
        Self {
            placements: Placements::NotManaged,
            publication: Publication::NotConfigured,
        }
    }
}
