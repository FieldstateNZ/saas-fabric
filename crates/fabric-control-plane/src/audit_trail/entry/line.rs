//! An entry's stored form: one line of JSON, bounded in size.

use crate::audit_trail::{AuditEntry, AuditError, MAX_AUDIT_ENTRY_BYTES};

impl AuditEntry {
    /// The entry as one line of JSON, without a trailing newline.
    ///
    /// # Errors
    ///
    /// [`AuditError::EntryTooLarge`] over [`MAX_AUDIT_ENTRY_BYTES`].
    pub fn encode(&self) -> Result<String, AuditError> {
        let line = serde_json::to_string(self).map_err(|error| AuditError::Malformed {
            detail: error.to_string(),
        })?;
        if line.len() > MAX_AUDIT_ENTRY_BYTES {
            return Err(AuditError::EntryTooLarge {
                size: line.len(),
                limit: MAX_AUDIT_ENTRY_BYTES,
            });
        }
        Ok(line)
    }

    /// Reads one line back. Says nothing about whether it is genuine.
    ///
    /// # Errors
    ///
    /// [`AuditError::Malformed`] if the line is not an entry, including one
    /// carrying a field an entry does not have.
    pub fn decode(line: &str) -> Result<Self, AuditError> {
        serde_json::from_str(line).map_err(|error| AuditError::Malformed {
            detail: error.to_string(),
        })
    }
}
