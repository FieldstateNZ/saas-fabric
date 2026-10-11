//! Why a publication was refused, or what is held could not be read.
//!
//! In the 121-150 line band: one enum, each variant naming the rule it
//! enforces. At the ceiling: trim rustdoc or find a real second concept.

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, TenantId};

use crate::{DocumentKind, DocumentRevision};

/// Why a publication was refused, or what is held could not be read. Every
/// variant but [`Self::Unwritable`] guarantees nothing was written.
#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    /// The offered revision is older than the one held.
    #[error("{document:?} is held at revision {held}, but an older revision {offered} was offered")]
    StaleRevision {
        /// Which document was stale.
        document: DocumentKind,
        /// The revision currently held.
        held: DocumentRevision,
        /// The older revision that was offered.
        offered: DocumentRevision,
    },

    /// The held revision offered with different bytes (ADR 0018 part 6).
    #[error("{document:?} at revision {revision} was offered with different bytes than are held")]
    DivergentPayload {
        /// Which document diverged.
        document: DocumentKind,
        /// The revision at which the bytes differ.
        revision: DocumentRevision,
    },

    /// A tenant binding names a DataSource this publication does not
    /// include: a certain 500 for that tenant (ADR 0018 part 4).
    #[error(
        "tenant {tenant}'s {logical} binding names data source {data_source}, which this \
         publication does not include"
    )]
    DanglingDataSource {
        /// The tenant whose binding names the missing DataSource.
        tenant: TenantId,
        /// Which of the tenant's logical bindings names it.
        logical: LogicalDataSourceName,
        /// The DataSource id no entry in the data-sources document matches.
        data_source: DataSourceId,
    },

    /// A DataSource the *held* tenants document binds is dropped: retiring
    /// one takes two publications (ADR 0018 part 3).
    #[error(
        "data source {data_source} was dropped, but the held tenants document still binds it to \
         tenant {tenant}"
    )]
    RetiredDataSourceStillBound {
        /// The DataSource this publication would retire.
        data_source: DataSourceId,
        /// A tenant, in the held tenants document, still bound to it.
        tenant: TenantId,
    },

    /// A non-empty document emptied without intent (ADR 0018 part 6).
    #[error("{document:?} would become empty, and Emptying::Intended was not given for it")]
    EmptyingNotIntended {
        /// The document that would have been emptied.
        document: DocumentKind,
    },

    /// The catalogue has no entries (ADR 0018 part 2).
    #[error("the catalogue document has no entries")]
    EmptyCatalogue,

    /// A tenant binding's `data` map is empty; the consumer drops it.
    #[error("tenant {tenant}'s data map has no entries, and would be dropped on arrival")]
    EmptyTenantData {
        /// The tenant whose binding has no data source bindings.
        tenant: TenantId,
    },

    /// A held manifest's payload is gone, disarming the guards that read it.
    #[error("{document:?}'s manifest is held, but its payload is gone -- restore the payload file or remove the manifest before publishing again")]
    HeldPayloadLost {
        /// The document whose payload is missing while its manifest remains.
        document: DocumentKind,
    },

    /// A tenant binding or DataSource offered older, changed at the same
    /// revision, or declared again at or below the revision it was last
    /// published at (O1, O3): a reader holding the higher one keeps it.
    #[error("{document:?} resource {resource} was published at revision {held} and is offered at {offered} without advancing")]
    ResourceRevisionNotAdvanced {
        /// The document the resource is in.
        document: DocumentKind,
        /// The tenant or DataSource id.
        resource: String,
        /// The highest revision it has been published at.
        held: BindingRevision,
        /// Its revision in this publication.
        offered: BindingRevision,
    },

    /// A held tenant dropped other than by intended emptying (O4, D01-27).
    #[error("tenant {tenant}, published at revision {held}, would be removed")]
    TenantRemoved {
        /// The tenant the held document binds and the offer drops.
        tenant: TenantId,
        /// Its revision in the held document.
        held: BindingRevision,
    },

    /// A catalogue entry on a logical data source no tenant binds (G8).
    #[error("catalogue resource {resource} is on {data_source}, which no tenant binds")]
    CatalogueEntryUnbound {
        /// The catalogue entry.
        resource: LogicalResourceName,
        /// The logical data source it names.
        data_source: LogicalDataSourceName,
    },

    /// The held record's rollback floor is above this publisher (D01-21).
    #[error(
        "the held publication record requires publisher contract {floor}; this publisher is {publisher}"
    )]
    BelowRollbackFloor {
        /// The held record's `minimum_publisher_contract`.
        floor: u32,
        /// This publisher's own contract version.
        publisher: u32,
    },

    /// A document or its manifest could not be read.
    #[error("{document:?} could not be read: {cause}")]
    Unreadable {
        /// The document that could not be read.
        document: DocumentKind,
        /// What went wrong. Never a credential — a filesystem or parse failure.
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },

    /// A document or its manifest could not be written.
    #[error("{document:?} could not be written: {cause}")]
    Unwritable {
        /// The document that could not be written.
        document: DocumentKind,
        /// What went wrong. Never a credential.
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },
}
