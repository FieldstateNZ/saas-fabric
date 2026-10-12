//! Why a restore was refused. Every refusal means nothing was written and
//! nothing was audited.

use crate::{DocumentKind, DocumentRevision};

/// Why [`super::plan_restore`] will not put the offered bytes back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RestoreRefusal {
    /// Only the tenants and data-sources documents are parsed before a
    /// publication; the catalogue is only byte-compared and never blocks.
    #[error("{document:?} is not restored by this command: only tenants and data sources are parsed before a publication")]
    NotRestorable {
        /// The document asked for.
        document: DocumentKind,
    },

    /// The document already parses: publication is not blocked by it, and
    /// its held bytes are left alone.
    #[error("{document:?} is not blocking publication; nothing to restore")]
    NotBlocked {
        /// The document asked for.
        document: DocumentKind,
    },

    /// No publication record is held, so nothing vouches for any bytes.
    #[error("no publication record is held, so no bytes can be shown to be the published ones")]
    NoRecord,

    /// The held record does not parse, or this publisher may not write over
    /// it (its rollback floor).
    #[error("the held publication record cannot be used: {detail}")]
    RecordUnusable {
        /// What is wrong with it, as a publication refusal would say it.
        detail: String,
    },

    /// The offered bytes are not the ones the record names.
    #[error(
        "the offered bytes have SHA-256 {offered}, but the held record names {recorded} for {document:?}"
    )]
    DigestMismatch {
        /// The document asked for.
        document: DocumentKind,
        /// The record's digest.
        recorded: String,
        /// The offered bytes' digest.
        offered: String,
    },

    /// The held manifest states a revision other than the record's: an
    /// interrupted or hand-edited publication, whose own bytes the record
    /// does not name. Restoring the record's bytes under it would hold
    /// content the manifest does not describe.
    #[error("{document:?}'s held manifest states revision {manifest}, but the held record names revision {recorded}")]
    ManifestDisagrees {
        /// The document asked for.
        document: DocumentKind,
        /// The held manifest's revision.
        manifest: DocumentRevision,
        /// The record's revision for it.
        recorded: DocumentRevision,
    },

    /// The offered bytes match the record's digest but do not parse as the
    /// document: the record itself names bytes this publisher cannot read.
    #[error("the offered bytes match the record but do not parse as {document:?}: {detail}")]
    OfferedUnparseable {
        /// The document asked for.
        document: DocumentKind,
        /// The parse error, as `PublicationError::Unreadable` carries it.
        detail: String,
    },
}
