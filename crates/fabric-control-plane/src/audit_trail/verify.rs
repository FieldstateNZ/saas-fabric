//! Proving a stored stream is the chain that was sealed.

use crate::audit_trail::{AuditEntry, AuditStream, ChainBreak, ChainHash, ChainHead};

/// Verifies `entries` as the whole of `stream`, in stored order, against the
/// head recorded for it.
///
/// Checks, entry by entry from the first: that it belongs to `stream`, that
/// its sequence is the next one, that it names the previous entry's hash, and
/// that its content still produces its own hash. Then, when `anchor` is
/// given, that the stream reaches the anchored entry and that entry's hash is
/// the anchored one.
///
/// Entries past the anchor are accepted when they chain correctly: an
/// adapter writes the entry before its head, so a stop between the two leaves
/// a stream one entry ahead of its anchor, which is a complete record and
/// not a tampered one.
///
/// Returns the stream's head, `None` for an empty stream.
///
/// # Errors
///
/// The first [`ChainBreak`] found.
pub fn verify(
    stream: &AuditStream,
    entries: &[AuditEntry],
    anchor: Option<&ChainHead>,
) -> Result<Option<ChainHead>, ChainBreak> {
    let mut previous = ChainHash::GENESIS;
    let mut expected = 1_u64;

    for (position, entry) in entries.iter().enumerate() {
        if entry.stream() != stream {
            return Err(ChainBreak::ForeignEntry {
                sequence: entry.sequence(),
            });
        }
        if entry.sequence() != expected {
            let found = entry.sequence();
            let later = entries
                .iter()
                .skip(position + 1)
                .any(|e| e.sequence() == expected);
            return Err(if later {
                ChainBreak::OutOfOrder { expected, found }
            } else {
                ChainBreak::Missing { expected, found }
            });
        }
        if entry.previous() != previous {
            return Err(ChainBreak::BrokenLink { sequence: expected });
        }
        if entry.expected_hash().ok() != Some(entry.hash()) {
            return Err(ChainBreak::Tampered { sequence: expected });
        }
        previous = entry.hash();
        expected = expected.saturating_add(1);
    }

    if let Some(anchor) = anchor {
        match entries.iter().find(|entry| entry.sequence() == anchor.sequence) {
            None => {
                return Err(ChainBreak::Truncated {
                    anchor: *anchor,
                    found: entries.last().map_or(0, AuditEntry::sequence),
                })
            }
            Some(entry) if entry.hash() != anchor.hash => {
                return Err(ChainBreak::AnchorMismatch { anchor: *anchor })
            }
            Some(_) => {}
        }
    }

    Ok(entries.last().map(AuditEntry::head))
}
