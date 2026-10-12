//! Why a set of resources could not be loaded from its source.

/// A source produced no usable state — it could not be read, could not be
/// understood, or held nothing that survived validation.
///
/// A load failure never clears a registry. The last good snapshot keeps
/// serving, because a momentarily unreadable source is a far better reason to
/// serve slightly stale state than to reject every request. On a *first* load
/// there is no snapshot to keep, so the registry is instead left unprimed —
/// returning 503 rather than pretending to be ready over an empty set.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// The source could not be read.
    ///
    /// The description field is named `origin` rather than `source` because
    /// `thiserror` treats a field called `source` as the error cause, and here
    /// it is only a human-readable label.
    #[error("could not read from {origin}")]
    Unreadable {
        /// A description of the source, for logs.
        origin: String,
        /// The underlying cause.
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The source was read but its contents could not be understood.
    #[error("contents of {origin} are malformed: {detail}")]
    Malformed {
        /// A description of the source, for logs.
        origin: String,
        /// What was wrong.
        detail: String,
    },

    /// The documents read do not match the publication record they were
    /// published under: a publication is in progress, was interrupted, or
    /// reached this reader one volume at a time. Holding the generation
    /// already served is the point -- applying either document alone would
    /// serve a mix of two generations.
    #[error("{document} in {origin} is not the one generation {generation} names; holding the last complete generation")]
    IncompleteGeneration {
        /// A description of the source, for logs.
        origin: String,
        /// The generation the record names.
        generation: u64,
        /// The first document whose bytes differ from the record's digest.
        document: &'static str,
    },

    /// The publication record says this generation needs a newer reader.
    /// The publisher raised its floor to activate a shape this reader does
    /// not know; refusing it keeps the last complete generation serving
    /// rather than misreading the new one.
    #[error("{origin} requires reader contract {required}; this reader is {supported}")]
    UnsupportedContract {
        /// A description of the source, for logs.
        origin: String,
        /// The record's `minimum_reader_contract`.
        required: u32,
        /// This reader's own contract.
        supported: u32,
    },

    /// A complete generation whose bytes match its record, but which could
    /// not be served whole: a document does not decode, a resource fails
    /// its own validation or appears twice, a binding names a DataSource
    /// the generation does not carry, or the catalogue does not decode.
    /// Raised only by a reader with a last-known-good directory, which
    /// never lets such a generation displace one it can serve.
    #[error("generation {generation} in {origin} cannot be served: {document}: {reason}; holding the last complete generation")]
    Unusable {
        /// A description of the source, for logs.
        origin: String,
        /// The record's generation.
        generation: u64,
        /// `tenants`, `data-sources` or `catalog`.
        document: &'static str,
        /// What is wrong with it.
        reason: String,
    },

    /// The published files offer less than this replica already holds: a
    /// complete generation below the one it served or persisted, or no
    /// record at all after a recorded generation was served. Runtime
    /// documents roll forward only (D01-1), so a lower generation is not a
    /// publication but a lost or reverted volume, and is held against.
    /// Raised only by a reader with a last-known-good directory.
    #[error("{origin} offers {found}, below generation {held} this replica holds; holding the last complete generation")]
    Regressed {
        /// A description of the source, for logs.
        origin: String,
        /// The highest generation this replica served or persisted.
        held: u64,
        /// What the published files offered instead.
        found: String,
    },

    /// The source parsed, but **every** resource in it failed validation, so
    /// there was nothing to install.
    ///
    /// Only ever produced by a *first* load
    /// ([`ResourceRefresher::prime`](crate::ResourceRefresher::prime)). A
    /// refresh cannot reach this state: by then a previous copy exists for
    /// every held key and is retained, so the registry keeps serving.
    ///
    /// It is a `SourceError` rather than a category of its own because that is
    /// what it is from the registry's side — a source that produced no usable
    /// state — and because it must reach the same decision the other variants
    /// do, [`fail_fast_on_prime`](crate::RuntimeConfig::fail_fast_on_prime).
    #[error("all {count} resources from {origin} failed validation; first was {reason}")]
    NothingUsable {
        /// A description of the source, for logs.
        origin: String,
        /// How many resources were published, every one of them unusable.
        count: usize,
        /// The first rejection, named, so the log says what to go and fix
        /// rather than only that something is wrong.
        reason: String,
    },
}
