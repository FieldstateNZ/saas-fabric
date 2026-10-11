//! The publication record: the file a publication writes last, naming the
//! exact bytes of each document that together form one generation.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): one
//! wire type and the held-record parse that enforces
//! the floor it carries -- splitting the floor check from the type that
//! records the floor would let one be changed without the other.

use crate::canonical::to_canonical_bytes;
use crate::{PublicationError, RecordedDocuments, CONTRACT_VERSION};

/// The publication record's file name, written beside `tenants.json` -- and,
/// in the Kubernetes adapter, a third key of the tenants `ConfigMap`, so it is
/// projected into the same volume, in the same symlink swap, as the tenants
/// payload it is written with.
pub const PUBLICATION_RECORD_FILE: &str = "publication.json";

/// The oldest reader contract that can serve what this publisher writes.
///
/// A reader whose own contract is below a record's
/// `minimum_reader_contract` refuses the generation and keeps its last
/// complete one; raising this constant is how a publisher activates a shape
/// only newer readers understand, once those readers are rolled out (#112:
/// compatible readers precede schema activation).
pub const MINIMUM_READER_CONTRACT: u32 = 1;

/// One publication generation: which revision, and which bytes, of each
/// document belong together.
///
/// Written after all three documents, so a reader that finds every document
/// matching the record's digests holds one complete generation, and a reader
/// that finds any mismatch is between two -- or after an interrupted
/// publication -- and keeps the generation it already serves.
///
/// `minimum_publisher_contract` is the rollback floor for the published
/// state (D01-21): a publisher whose [`CONTRACT_VERSION`] is below it must
/// not write over this record, because it would leave readers a record it
/// does not understand. Every publisher from this one on enforces it; a
/// publisher older than the record cannot, which is why the release that
/// introduced the record is itself the floor for rolling the publisher back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRecord {
    contract_version: u32,
    minimum_reader_contract: u32,
    minimum_publisher_contract: u32,
    generation: u64,
    documents: RecordedDocuments,
}

/// Only the floor, read before anything else in a held record, so a record
/// written under a newer contract is refused by its floor rather than by an
/// unknown field.
#[derive(serde::Deserialize)]
struct Floor {
    minimum_publisher_contract: u32,
}

impl PublicationRecord {
    /// A record at `generation` naming `documents`, stamped with this
    /// publisher's contract and floors.
    #[must_use]
    pub const fn new(generation: u64, documents: RecordedDocuments) -> Self {
        Self {
            contract_version: CONTRACT_VERSION,
            minimum_reader_contract: MINIMUM_READER_CONTRACT,
            minimum_publisher_contract: CONTRACT_VERSION,
            generation,
            documents,
        }
    }

    /// The generation number: one more than the record it replaced.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// The documents this generation names.
    #[must_use]
    pub const fn documents(&self) -> &RecordedDocuments {
        &self.documents
    }

    /// The oldest reader contract that may serve this generation.
    #[must_use]
    pub const fn minimum_reader_contract(&self) -> u32 {
        self.minimum_reader_contract
    }

    /// The oldest publisher contract that may write over this record.
    #[must_use]
    pub const fn minimum_publisher_contract(&self) -> u32 {
        self.minimum_publisher_contract
    }

    /// Canonical JSON, the formatting rule every published file shares.
    ///
    /// # Errors
    ///
    /// Only if this type's own `Serialize` fails, which it never does.
    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        to_canonical_bytes(self)
    }

    /// Reads a held record, refusing one whose rollback floor is above this
    /// publisher's contract before trying to understand the rest of it.
    ///
    /// # Errors
    ///
    /// [`PublicationError::BelowRollbackFloor`] for a record this publisher
    /// may not write over; [`PublicationError::Unreadable`] (as the tenants
    /// document, which the record travels with) for one that does not parse.
    pub fn parse_held(bytes: &[u8]) -> Result<Self, PublicationError> {
        let floor: Floor = serde_json::from_slice(bytes).map_err(unreadable)?;
        if floor.minimum_publisher_contract > CONTRACT_VERSION {
            return Err(PublicationError::BelowRollbackFloor {
                floor: floor.minimum_publisher_contract,
                publisher: CONTRACT_VERSION,
            });
        }
        serde_json::from_slice(bytes).map_err(unreadable)
    }
}

fn unreadable(cause: serde_json::Error) -> PublicationError {
    PublicationError::Unreadable {
        document: crate::DocumentKind::Tenants,
        cause: Box::new(cause),
    }
}
