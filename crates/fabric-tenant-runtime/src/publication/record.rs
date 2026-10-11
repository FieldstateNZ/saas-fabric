//! The reader's copy of the publication record.

/// The contract this reader implements. A record whose
/// `minimum_reader_contract` is above it is refused, and the last complete
/// generation keeps serving.
pub const READER_CONTRACT: u32 = 1;

/// The record's file name, beside `tenants.json` -- in Kubernetes, a key of
/// the tenants `ConfigMap`, projected in the same volume swap as the
/// tenants payload.
pub(super) const PUBLICATION_RECORD_FILE: &str = "publication.json";

/// What a reader needs from a publication record, and nothing else.
///
/// Unlike the documents, this does **not** deny unknown fields. The
/// documents rely on `deny_unknown_fields` to refuse a shape they do not
/// know; the record carries an explicit floor instead, so a publisher can
/// add to the record what older readers may ignore and raise
/// `minimum_reader_contract` only when they may not.
#[derive(Debug, serde::Deserialize)]
pub(super) struct ReaderRecord {
    pub(super) minimum_reader_contract: u32,
    pub(super) generation: u64,
    pub(super) documents: ReaderDocuments,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct ReaderDocuments {
    pub(super) tenants: ReaderDocument,
    #[serde(rename = "data-sources")]
    pub(super) data_sources: ReaderDocument,
    /// Checked once, when the catalogue is read at process start.
    pub(super) catalog: ReaderDocument,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct ReaderDocument {
    pub(super) sha256: String,
}
