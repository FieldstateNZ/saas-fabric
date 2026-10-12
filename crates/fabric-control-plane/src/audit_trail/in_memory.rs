//! An audit trail that lives as long as the process.

use std::collections::BTreeMap;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::audit_trail::{AuditEntry, AuditError, AuditRecord, AuditStream, AuditTrail, StoredStream};

/// The in-memory [`AuditTrail`], for tests and for proving the port's rules
/// without a filesystem.
///
/// Not a deployment option: a trail that forgets on restart is not durable,
/// and the host offers no configuration that selects it.
#[derive(Default)]
pub struct InMemoryAuditTrail {
    streams: Mutex<BTreeMap<AuditStream, Vec<AuditEntry>>>,
}

impl InMemoryAuditTrail {
    /// An empty trail.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AuditTrail for InMemoryAuditTrail {
    async fn append(&self, record: AuditRecord, at: u64) -> Result<AuditEntry, AuditError> {
        let mut streams = self.streams.lock().await;
        let entries = streams.entry(record.stream().clone()).or_default();
        let head = entries.last().map(AuditEntry::head);
        let entry = AuditEntry::seal(head.as_ref(), at, record)?;
        entries.push(entry.clone());
        Ok(entry)
    }

    async fn read(&self, stream: &AuditStream) -> Result<StoredStream, AuditError> {
        let entries = self.streams.lock().await.get(stream).cloned().unwrap_or_default();
        Ok(StoredStream {
            stream: stream.clone(),
            anchor: entries.last().map(AuditEntry::head),
            entries,
        })
    }

    fn describe(&self) -> String {
        "an in-memory audit trail".to_owned()
    }
}
