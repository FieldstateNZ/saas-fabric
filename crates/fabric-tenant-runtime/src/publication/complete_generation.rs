//! One generation's exact bytes, checked against the record that names them.
//!
//! The published files and the persisted last-known-good copy are checked
//! by the same rule, so the copy can never be held to a weaker standard
//! than what the publisher wrote: the record's floor admits this reader, and
//! every document's SHA-256 is the one the record names.

use fabric_core::RecordDigest;

use super::digest::{sha256_digest, sha256_hex};
use super::file_reads::parse;
use super::record::{ReaderRecord, READER_CONTRACT};
use crate::SourceError;

/// A record and the documents it names, byte for byte.
#[derive(Debug, Clone)]
pub(crate) struct CompleteGeneration {
    pub(crate) record: Vec<u8>,
    pub(crate) generation: u64,
    pub(crate) data_sources: Vec<u8>,
    pub(crate) tenants: Vec<u8>,
    pub(crate) catalog: Vec<u8>,
}

impl CompleteGeneration {
    /// Checks `record` and the documents against it.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] for a record that does not parse,
    /// [`SourceError::UnsupportedContract`] for a floor above this reader,
    /// and [`SourceError::IncompleteGeneration`] naming the first document
    /// whose bytes differ from the record's digest. All three must match:
    /// a catalogue that lags its record holds the whole generation (G8).
    pub(crate) fn verify(
        record: Vec<u8>,
        data_sources: Vec<u8>,
        tenants: Vec<u8>,
        catalog: Vec<u8>,
        origin: &str,
    ) -> Result<Self, SourceError> {
        let parsed: ReaderRecord = parse(&record, origin)?;
        if parsed.minimum_reader_contract > READER_CONTRACT {
            return Err(SourceError::UnsupportedContract {
                origin: origin.to_owned(),
                required: parsed.minimum_reader_contract,
                supported: READER_CONTRACT,
            });
        }
        let documents = &parsed.documents;
        let named = [
            ("catalog", catalog.as_slice(), &documents.catalog.sha256),
            (
                "data-sources",
                data_sources.as_slice(),
                &documents.data_sources.sha256,
            ),
            ("tenants", tenants.as_slice(), &documents.tenants.sha256),
        ];
        for (document, bytes, sha256) in named {
            if sha256_hex(bytes) != *sha256 {
                return Err(SourceError::IncompleteGeneration {
                    origin: origin.to_owned(),
                    generation: parsed.generation,
                    document,
                });
            }
        }
        Ok(Self {
            generation: parsed.generation,
            record,
            data_sources,
            tenants,
            catalog,
        })
    }

    /// The SHA-256 of the record's own bytes, which identifies the
    /// generation exactly: two records at one generation number that differ
    /// are two different generations.
    pub(crate) fn record_sha256(&self) -> String {
        sha256_hex(&self.record)
    }

    /// [`Self::record_sha256`], as the digest a replica reports serving.
    pub(crate) fn record_digest(&self) -> RecordDigest {
        sha256_digest(&self.record)
    }
}
