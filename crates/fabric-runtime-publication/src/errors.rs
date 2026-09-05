//! Why a publication could not do what was asked, or why what is currently
//! held could not be read.
//!
//! This file is in the 121-150 line band the file-size policy asks a reason
//! for: it is one enum, and every variant needs its own rustdoc explaining
//! which ADR 0018 rule it enforces and why that rule refuses rather than
//! degrades. Splitting the enum from any of its variants' documentation
//! would separate a refusal from the reasoning a caller needs to act on it.

use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};

use crate::{DocumentKind, DocumentRevision};

/// Why [`crate::RuntimePublication::publish`] refused a publication, or
/// [`crate::RuntimePublication::current`] could not read what is held.
///
/// # Refused, never partially applied
///
/// Every variant here means the whole publication was refused before the
/// first byte was written (ADR 0018 parts 4-6). There is no variant for "two
/// of three documents were written and the third failed", because that state
/// is unreachable by construction: every document's verdict is resolved
/// before any of them is written.
#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    /// The offered revision is older than the one already held.
    ///
    /// A `409`-shaped outcome: read the current revisions and try again.
    #[error("{document:?} is held at revision {held}, but an older revision {offered} was offered")]
    StaleRevision {
        /// Which document was stale.
        document: DocumentKind,
        /// The revision currently held.
        held: DocumentRevision,
        /// The older revision that was offered.
        offered: DocumentRevision,
    },

    /// The offered revision matches what is held, but the bytes differ.
    ///
    /// Refused rather than accepted, deliberately: accepting the newer bytes
    /// at an unchanged revision would make the revision meaningless, and two
    /// writers racing at the same revision would have their outcome decided
    /// by arrival order (ADR 0018 part 6, mirroring the runtime's own
    /// `ApplyReport::divergent_payload`).
    #[error("{document:?} at revision {revision} was offered with different bytes than are held")]
    DivergentPayload {
        /// Which document diverged.
        document: DocumentKind,
        /// The revision at which the bytes differ.
        revision: DocumentRevision,
    },

    /// A tenant binding names a `DataSourceId` this same publication's
    /// data-sources document does not contain.
    ///
    /// Guaranteed to produce a 500 on the request path for that tenant
    /// (`ResolveError::MissingDataSource`) — cheap to catch here instead of
    /// on a live request (ADR 0018 part 4).
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
    /// document — the one currently on disk, not the one in this
    /// publication — still references.
    ///
    /// Retiring a DataSource is therefore two publications: one that unbinds
    /// every tenant from it, then a second, once the first is held, that
    /// drops the DataSource itself (ADR 0018 part 3).
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

    /// This publication would take a currently non-empty document to empty,
    /// and the caller did not state that intent.
    ///
    /// Prevents a scheduled publication whose input query returned zero rows
    /// from silently deprovisioning every tenant (ADR 0018 part 6). State
    /// [`crate::Emptying::Intended`] for this document if that is really
    /// what is meant.
    #[error("{document:?} would become empty, and Emptying::Intended was not given for it")]
    EmptyingNotIntended {
        /// The document that would have been emptied.
        document: DocumentKind,
    },

    /// The catalogue document has no entries.
    ///
    /// There is no bootstrap value for an empty catalogue — unlike tenants
    /// and data sources, `[]` is never legitimate here, because
    /// `build_data_api` refuses to start against one (ADR 0018 part 2).
    /// Refused unconditionally, whatever the [`crate::Emptying`] intent says.
    #[error("the catalogue document has no entries")]
    EmptyCatalogue,

    /// A document or its manifest could not be read.
    #[error("{document:?} could not be read: {cause}")]
    Unreadable {
        /// The document that could not be read.
        document: DocumentKind,
        /// What went wrong. Never a credential — this is a filesystem or
        /// parse failure.
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
