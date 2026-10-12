//! One generation's exact bytes, checked against the record that names them.
//!
//! The published files and the persisted last-known-good copy are checked
//! by the same rule, so the copy can never be held to a weaker standard
//! than what the publisher wrote: the record's floor admits this reader, and
//! every document's SHA-256 is the one the record names.

use super::digest::sha256_hex;
use super::file_reads::parse;
use super::record::{ReaderRecord, READER_CONTRACT};
use crate::{DataSource, SourceError, TenantRuntimeBinding};

/// A record and the documents it names, byte for byte.
#[derive(Debug, Clone)]
pub(crate) struct CompleteGeneration {
    pub(crate) record: Vec<u8>,
    pub(crate) generation: u64,
    pub(crate) data_sources: Vec<u8>,
    pub(crate) tenants: Vec<u8>,
    /// Present only when read and matching the record's digest.
    pub(crate) catalog: Option<Vec<u8>>,
    catalog_sha256: String,
}

impl CompleteGeneration {
    /// Checks `record` and the documents against it.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] for a record that does not parse,
    /// [`SourceError::UnsupportedContract`] for a floor above this reader,
    /// and [`SourceError::IncompleteGeneration`] naming the first document
    /// whose bytes differ from the record's digest. A `catalog` given here
    /// must match; see [`Self::attach_catalogue_if_matching`] for one that
    /// may not.
    pub(crate) fn verify(
        record: Vec<u8>,
        data_sources: Vec<u8>,
        tenants: Vec<u8>,
        catalog: Option<Vec<u8>>,
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
            ("catalog", catalog.as_deref(), &documents.catalog.sha256),
            (
                "data-sources",
                Some(data_sources.as_slice()),
                &documents.data_sources.sha256,
            ),
            ("tenants", Some(tenants.as_slice()), &documents.tenants.sha256),
        ];
        for (document, bytes, sha256) in named {
            if bytes.is_some_and(|bytes| sha256_hex(bytes) != *sha256) {
                return Err(SourceError::IncompleteGeneration {
                    origin: origin.to_owned(),
                    generation: parsed.generation,
                    document,
                });
            }
        }
        Ok(Self {
            generation: parsed.generation,
            catalog_sha256: parsed.documents.catalog.sha256,
            record,
            data_sources,
            tenants,
            catalog,
        })
    }

    /// Keeps `catalog` only if it is the one the record names. A catalogue
    /// that lags the record is not an error here: the caller wanted it
    /// opportunistically, to persist the generation whole.
    pub(crate) fn attach_catalogue_if_matching(self, catalog: Option<Vec<u8>>) -> Self {
        let catalog = catalog.filter(|bytes| sha256_hex(bytes) == self.catalog_sha256);
        Self { catalog, ..self }
    }

    /// The SHA-256 of the record's own bytes, which identifies the
    /// generation exactly: two records at one generation number that differ
    /// are two different generations.
    pub(crate) fn record_sha256(&self) -> String {
        sha256_hex(&self.record)
    }

    /// The two documents, parsed.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] if either does not parse.
    pub(crate) fn parse_documents(
        &self,
        origin: &str,
    ) -> Result<(Vec<DataSource>, Vec<TenantRuntimeBinding>), SourceError> {
        Ok((parse(&self.data_sources, origin)?, parse(&self.tenants, origin)?))
    }
}
