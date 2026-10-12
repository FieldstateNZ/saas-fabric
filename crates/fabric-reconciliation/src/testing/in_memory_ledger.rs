//! A declaration ledger held in memory, for tests on both sides of the
//! crate boundary.
//!
//! Process-only, which is exactly what the `ledger` module says production
//! must not trust; it stands in for a durable one so a test can drive
//! classification and recording, and refuse on cue.
//!
//! In the 121–150 line band: one fake, its failure switches and its port
//! implementation, which a test drives together.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabric_client_model::{Client, ClientId, ClientRevision, OidcClient, OidcClientId, RealmName};

use crate::ledger::{
    callback_digests, AppliedBaseline, CallbackDigest, DeclarationLedger, LedgerError, WriteStage,
};

use super::fake_identity_provider::lock;
use super::ledger_at_revision::AtRevision;

/// One stored record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerRecord {
    /// The client whose stream holds it.
    pub client: ClientId,
    /// The realm it was written in.
    pub realm: RealmName,
    /// The declared client written.
    pub declared: OidcClientId,
    /// Its callbacks, by digest.
    pub callbacks: BTreeSet<CallbackDigest>,
    /// Which side of the write.
    pub stage: WriteStage,
    /// The revision the recording pass converged, when it was scoped to one.
    pub revision: Option<ClientRevision>,
}

/// See this module's rustdoc.
#[derive(Default)]
pub struct InMemoryDeclarationLedger {
    records: Mutex<Vec<LedgerRecord>>,
    refusing: Mutex<Option<WriteStage>>,
    unreadable: Mutex<bool>,
}

impl InMemoryDeclarationLedger {
    /// An empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every record, in stored order.
    #[must_use]
    pub fn records(&self) -> Vec<LedgerRecord> {
        lock(&self.records).clone()
    }

    /// Refuses every later record at `stage`, or none.
    pub fn refuse(&self, stage: Option<WriteStage>) {
        *lock(&self.refusing) = stage;
    }

    /// Makes every later read fail, or succeed again.
    pub fn set_unreadable(&self, unreadable: bool) {
        *lock(&self.unreadable) = unreadable;
    }

    /// This ledger, stamping every record with `revision`: what a pass
    /// converging that revision is handed.
    #[must_use]
    pub fn at(self: &Arc<Self>, revision: ClientRevision) -> Arc<dyn DeclarationLedger> {
        Arc::new(AtRevision {
            ledger: Arc::clone(self),
            revision,
        })
    }

    /// Forgets everything, as a deployment with a fresh store would.
    pub fn clear(&self) {
        lock(&self.records).clear();
    }
}

#[async_trait]
impl DeclarationLedger for InMemoryDeclarationLedger {
    async fn read(&self, client: &Client) -> Result<AppliedBaseline, LedgerError> {
        if *lock(&self.unreadable) {
            return Err(LedgerError::Unreadable {
                detail: "refused by the test".to_owned(),
            });
        }

        let mut baseline = AppliedBaseline::unknown();
        for record in self.records() {
            if record.client == client.id && record.realm == client.identity.realm {
                baseline.replay(record.declared, record.callbacks, record.stage);
            }
        }
        Ok(baseline)
    }

    async fn record(
        &self,
        client: &Client,
        declared: &OidcClient,
        stage: WriteStage,
    ) -> Result<(), LedgerError> {
        self.append(client, declared, stage, None)
    }
}

impl InMemoryDeclarationLedger {
    pub(super) fn append(
        &self,
        client: &Client,
        declared: &OidcClient,
        stage: WriteStage,
        revision: Option<ClientRevision>,
    ) -> Result<(), LedgerError> {
        if *lock(&self.refusing) == Some(stage) {
            return Err(LedgerError::Unrecorded {
                detail: "refused by the test".to_owned(),
            });
        }

        lock(&self.records).push(LedgerRecord {
            client: client.id.clone(),
            realm: client.identity.realm.clone(),
            declared: declared.id.clone(),
            callbacks: callback_digests(declared),
            stage,
            revision,
        });
        Ok(())
    }
}
