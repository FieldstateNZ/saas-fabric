//! Why a restore was refused. Every refusal means nothing was written and
//! no intent was recorded.

use crate::{DocumentKind, DocumentRevision};

/// Why a restore will not put the offered bytes back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RestoreRefusal {
    /// Only tenants and data sources are parsed before a publication.
    #[error("{document:?} is not restored by this command: only tenants and data sources are parsed before a publication")]
    NotRestorable {
        /// The document asked for.
        document: DocumentKind,
    },

    /// No exclusive, fenced ownership of the target is established, so no
    /// write can be bound to the record that authorises it.
    #[error("this target has no established exclusive publisher ownership; only inspect and validate are supported")]
    OwnershipUnestablished,

    /// The record moved while the target was read.
    #[error("the publication record changed while the target was read; nothing was planned against it")]
    Unstable,

    /// The held payload parses and is not the recorded bytes: readable
    /// content the record does not name, which a restore never replaces.
    #[error("{document:?}'s held payload is readable and is not the recorded bytes; nothing to restore")]
    HeldReadable {
        /// The document asked for.
        document: DocumentKind,
    },

    /// No publication record is held.
    #[error("no publication record is held, so no bytes can be matched to it")]
    NoRecord,

    /// The held record does not parse, or its floor is above this publisher.
    #[error("the held publication record cannot be used: {detail}")]
    RecordUnusable {
        /// Why, as a publication refusal says it.
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

    /// The held manifest is readable and states another revision.
    #[error("{document:?}'s held manifest states revision {manifest}, but the held record names revision {recorded}")]
    ManifestDisagrees {
        /// The document asked for.
        document: DocumentKind,
        /// The held manifest's revision.
        manifest: DocumentRevision,
        /// The record's revision for it.
        recorded: DocumentRevision,
    },

    /// The held manifest is readable metadata a restore may not replace.
    #[error("{document:?}'s held manifest is readable and conflicts: {detail}")]
    ManifestConflicts {
        /// The document asked for.
        document: DocumentKind,
        /// Why.
        detail: String,
    },

    /// The offered bytes match the record but do not parse.
    #[error("the offered bytes match the record but do not parse as {document:?}: {detail}")]
    OfferedUnparseable {
        /// The document asked for.
        document: DocumentKind,
        /// The parse error.
        detail: String,
    },

    /// The offered bytes match the record and parse, but break a resource
    /// rule, alone or against a usable held peer.
    #[error("the offered {document:?} match the record but are not usable: {}", problems.join("; "))]
    OfferedInvalid {
        /// The document asked for.
        document: DocumentKind,
        /// Each rule broken.
        problems: Vec<String>,
    },
}
