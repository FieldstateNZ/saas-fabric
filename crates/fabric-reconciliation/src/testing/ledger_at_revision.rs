//! [`InMemoryDeclarationLedger`], scoped to the revision a pass converges.

use std::sync::Arc;

use async_trait::async_trait;
use fabric_client_model::{Client, ClientRevision, OidcClient};

use super::InMemoryDeclarationLedger;
use crate::ledger::{AppliedBaseline, DeclarationLedger, LedgerError, WriteStage};

/// [`InMemoryDeclarationLedger::at`].
pub(super) struct AtRevision {
    pub(super) ledger: Arc<InMemoryDeclarationLedger>,
    pub(super) revision: ClientRevision,
}

#[async_trait]
impl DeclarationLedger for AtRevision {
    async fn read(&self, client: &Client) -> Result<AppliedBaseline, LedgerError> {
        self.ledger.read(client).await
    }

    async fn record(
        &self,
        client: &Client,
        declared: &OidcClient,
        stage: WriteStage,
    ) -> Result<(), LedgerError> {
        self.ledger
            .append(client, declared, stage, Some(self.revision.clone()))
    }
}
