//! The last-known-good file's on-disk shape.

/// The only format this reader writes or reads.
pub(super) const FORMAT: u32 = 1;

/// One complete generation in one file, so one rename replaces it whole.
///
/// Every document is kept as the exact text that was published, because the
/// record's digests are over those bytes: re-serialising a parsed document
/// would change them and the copy could never be checked. `record_sha256`
/// covers the record itself, which nothing else does.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Envelope {
    pub(super) format: u32,
    pub(super) record_sha256: String,
    pub(super) record: String,
    pub(super) data_sources: String,
    pub(super) tenants: String,
    pub(super) catalog: String,
}
