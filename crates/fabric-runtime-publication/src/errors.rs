//! Why a publication was refused, or what is held could not be read.
//!
//! In the 121-150 line band: one enum, each variant naming the rule it
//! enforces. At the ceiling: trim rustdoc or find a real second concept.

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};

use crate::{DocumentKind, DocumentRevision};

/// Why [`crate::RuntimePublication::publish`] refused a publication, or
/// [`crate::RuntimePublication::current`] could not read what is held.
///
/// Every variant but [`Self::Unwritable`] guarantees nothing was written --
/// see [`crate::PublicationReport`] for what a partial write there means.
#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    /// The offered revision is older than the one already held: read the
    /// current revisions and try again.
    #[error("{document:?} is held at revision {held}, but an older revision {offered} was offered")]
    StaleRevision {
        /// Which document was stale.
        document: DocumentKind,
        /// The revision currently held.
        held: DocumentRevision,
        /// The older revision that was offered.
        offered: DocumentRevision,
    },

    /// The offered revision matches what is held, but the bytes differ:
    /// accepting them would make the revision meaningless (ADR 0018 part 6).
    #[error("{document:?} at revision {revision} was offered with different bytes than are held")]
    DivergentPayload {
        /// Which document diverged.
        document: DocumentKind,
        /// The revision at which the bytes differ.
        revision: DocumentRevision,
    },

    /// A tenant binding names a `DataSourceId` this same publication's
    /// data-sources document does not contain: a certain 500 for that
    /// tenant (ADR 0018 part 4).
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

    /// The data-sources document drops a `DataSourceId` the *held* tenants
    /// document still references: retiring a DataSource takes two
    /// publications (ADR 0018 part 3).
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

    /// This publication would take a currently non-empty document to empty
    /// without [`crate::Emptying::Intended`] (ADR 0018 part 6).
    #[error("{document:?} would become empty, and Emptying::Intended was not given for it")]
    EmptyingNotIntended {
        /// The document that would have been emptied.
        document: DocumentKind,
    },

    /// The catalogue document has no entries; `build_data_api` refuses to
    /// start against one (ADR 0018 part 2).
    #[error("the catalogue document has no entries")]
    EmptyCatalogue,

    /// A tenant binding's `data` map has no entries, which the consumer
    /// drops on arrival.
    #[error("tenant {tenant}'s data map has no entries, and would be dropped on arrival")]
    EmptyTenantData {
        /// The tenant whose binding has no data source bindings.
        tenant: TenantId,
    },

    /// A tenants or data-sources manifest is held, but its payload is gone,
    /// disarming whichever guard reads it.
    #[error(
        "{document:?}'s manifest is held, but its payload is gone -- restore the payload file or \
         remove the manifest before publishing again"
    )]
    HeldPayloadLost {
        /// The document whose payload is missing while its manifest remains.
        document: DocumentKind,
    },

    /// A tenant binding or DataSource the held document already holds is
    /// offered at an older revision, or changed at the same one. A reader
    /// keeps its held copy of such a resource and applies the rest, so
    /// publishing it would serve a generation only part-applied.
    #[error("{document:?} resource {resource} is held at revision {held} and offered at {offered} without advancing")]
    ResourceRevisionNotAdvanced {
        /// The document the resource is in.
        document: DocumentKind,
        /// The tenant or DataSource id.
        resource: String,
        /// Its revision in the held document.
        held: BindingRevision,
        /// Its revision in this publication.
        offered: BindingRevision,
    },

    /// The held publication record was written under a contract whose
    /// rollback floor is above this publisher's (D01-21): writing over it
    /// would hand readers a record this publisher does not understand.
    /// Roll the publisher forward to at least `floor`.
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
