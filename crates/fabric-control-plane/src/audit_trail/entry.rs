//! An audit record sealed into its stream's chain. Its line form is in
//! `entry/line.rs`. In the 121–150 band: one type, its sealing, and the
//! read-only accessors that are the only way to see its fields.

mod line;

use crate::audit_trail::{AuditError, AuditRecord, AuditStream, ChainHash, ChainHead};

/// One link of a stream's chain: a record, where it sits, and the hash that
/// binds it to everything before it.
///
/// Built only by [`seal`](Self::seal), which computes the hash, or by
/// [`decode`](Self::decode), which reads one back without trusting it —
/// [`verify`](crate::audit_trail::verify) is what decides whether a decoded
/// entry is what was sealed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEntry {
    sequence: u64,
    at: u64,
    previous: ChainHash,
    record: AuditRecord,
    hash: ChainHash,
}

/// The bytes an entry's hash is taken over: everything but the hash itself.
///
/// Field order is fixed by this declaration and `AuditRecord`'s fields are a
/// sorted map, so the same entry always serialises to the same bytes.
#[derive(serde::Serialize)]
struct Sealed<'a> {
    sequence: u64,
    at: u64,
    previous: &'a ChainHash,
    record: &'a AuditRecord,
}

impl AuditEntry {
    /// Seals `record` as the entry after `head` (`None` for a stream's first).
    ///
    /// # Errors
    ///
    /// [`AuditError::EntryTooLarge`] if the encoded entry would exceed
    /// [`MAX_AUDIT_ENTRY_BYTES`](crate::MAX_AUDIT_ENTRY_BYTES). The record is refused whole, never
    /// shortened. [`AuditError::Unavailable`] if the sequence would overflow.
    pub fn seal(head: Option<&ChainHead>, at: u64, record: AuditRecord) -> Result<Self, AuditError> {
        let (sequence, previous) = match head {
            None => (1, ChainHash::GENESIS),
            Some(head) => (
                head.sequence
                    .checked_add(1)
                    .ok_or_else(|| AuditError::Unavailable {
                        detail: "the stream's sequence is exhausted".to_owned(),
                    })?,
                head.hash,
            ),
        };
        let mut entry = Self {
            sequence,
            at,
            previous,
            record,
            hash: ChainHash::GENESIS,
        };
        entry.hash = entry.expected_hash()?;
        entry.encode()?;
        Ok(entry)
    }

    /// The hash this entry's content implies, recomputed from scratch.
    pub(crate) fn expected_hash(&self) -> Result<ChainHash, AuditError> {
        let sealed = Sealed {
            sequence: self.sequence,
            at: self.at,
            previous: &self.previous,
            record: &self.record,
        };
        serde_json::to_vec(&sealed)
            .map(|bytes| ChainHash::of(&bytes))
            .map_err(|error| AuditError::Malformed {
                detail: error.to_string(),
            })
    }

    /// The stream this entry belongs to.
    #[must_use]
    pub fn stream(&self) -> &AuditStream {
        self.record.stream()
    }

    /// Its position in the stream, counting from one.
    #[must_use]
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// When it was recorded, Unix seconds.
    #[must_use]
    pub fn at(&self) -> u64 {
        self.at
    }

    /// The hash of the entry before it.
    #[must_use]
    pub fn previous(&self) -> ChainHash {
        self.previous
    }

    /// What was recorded.
    #[must_use]
    pub fn record(&self) -> &AuditRecord {
        &self.record
    }

    /// This entry's hash, as stored.
    #[must_use]
    pub fn hash(&self) -> ChainHash {
        self.hash
    }

    /// This entry as a chain head.
    #[must_use]
    pub fn head(&self) -> ChainHead {
        ChainHead {
            sequence: self.sequence,
            hash: self.hash,
        }
    }
}
