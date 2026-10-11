//! The publication record: the file a publication writes last, naming the
//! exact bytes of each document that together form one generation.
//!
//! In the 121-150 band: one wire type and the held-record parse that
//! enforces the floor it carries -- split, one could change without the other.

use crate::canonical::to_canonical_bytes;
use crate::rollback_floor::check_publisher_floor;
use crate::{PublicationError, RecordedDocuments, RecordedRegistry, ResourceHistory, CONTRACT_VERSION};

/// The record's file name: beside `tenants.json`, and in Kubernetes a key of
/// the tenants `ConfigMap`, projected in the same swap as its payload.
pub const PUBLICATION_RECORD_FILE: &str = "publication.json";

/// The oldest reader contract that can serve what this publisher writes. A
/// reader below a record's `minimum_reader_contract` keeps its last complete
/// generation; raising this activates a shape only newer readers understand,
/// once they are rolled out (#112). The registry does not raise it: readers
/// ignore record keys they do not know, and no runtime reads the registry.
pub const MINIMUM_READER_CONTRACT: u32 = 1;

/// One publication generation: which revision, and which bytes, of each
/// document belong together.
///
/// Written after all three documents, so a reader that finds every document
/// matching its digests holds one complete generation, and one that finds a
/// mismatch is between two and keeps the generation it serves.
///
/// `minimum_publisher_contract` is the rollback floor (D01-21): a publisher
/// whose [`CONTRACT_VERSION`] is below it must not write over this record. A
/// publisher older than the record cannot enforce that, which is why the
/// release that introduced the record is the floor for rolling back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRecord {
    contract_version: u32,
    minimum_reader_contract: u32,
    minimum_publisher_contract: u32,
    generation: u64,
    documents: RecordedDocuments,
    /// Absent before #136, reading as empty. A publisher from before it
    /// refuses a record that carries one as unreadable, rather than drop it.
    #[serde(default)]
    resources: ResourceHistory,
    /// The issuer registry this generation carries (D01-4); absent, not
    /// `null`, until one is published, so earlier records keep their bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    registry: Option<RecordedRegistry>,
}

impl PublicationRecord {
    /// A record at `generation` naming `documents` and the resource history
    /// up to and including them, stamped with this publisher's contract and
    /// floors.
    #[must_use]
    pub const fn new(generation: u64, documents: RecordedDocuments, resources: ResourceHistory) -> Self {
        Self {
            contract_version: CONTRACT_VERSION,
            minimum_reader_contract: MINIMUM_READER_CONTRACT,
            minimum_publisher_contract: CONTRACT_VERSION,
            generation,
            documents,
            resources,
            registry: None,
        }
    }

    /// This record, naming `registry` as the generation's issuer registry.
    #[must_use]
    pub fn with_registry(self, registry: Option<RecordedRegistry>) -> Self {
        Self { registry, ..self }
    }

    /// The issuer registry this generation carries, if one is published.
    #[must_use]
    pub const fn registry(&self) -> Option<&RecordedRegistry> {
        self.registry.as_ref()
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

    /// The highest revision each resource has been published at.
    #[must_use]
    pub const fn resources(&self) -> &ResourceHistory {
        &self.resources
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

    /// Canonical JSON.
    ///
    /// # Errors
    ///
    /// Only if this type's own `Serialize` fails, which it never does.
    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        to_canonical_bytes(self)
    }

    /// Reads a held record, refusing one above this publisher's floor first.
    ///
    /// # Errors
    ///
    /// [`PublicationError::BelowRollbackFloor`] for a record this publisher
    /// may not write over; [`PublicationError::Unreadable`] (as the tenants
    /// document, which the record travels with) for one that does not parse.
    pub fn parse_held(bytes: &[u8]) -> Result<Self, PublicationError> {
        check_publisher_floor(bytes, CONTRACT_VERSION)?;
        serde_json::from_slice(bytes).map_err(unreadable)
    }
}

pub(crate) fn unreadable(cause: serde_json::Error) -> PublicationError {
    PublicationError::Unreadable {
        document: crate::DocumentKind::Tenants,
        cause: Box::new(cause),
    }
}
