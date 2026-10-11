//! The Check a deployment has when it has none.

use async_trait::async_trait;

use crate::{AuthorizationCheck, CallerCredential, CheckDecision, CheckFailure, CheckQuestion};

/// Answers every question with [`CheckFailure::NotConfigured`].
///
/// The default, because OpenFGA is not deployed and no front endpoint is
/// configured (D01-10a). It is not "allow": a tenant policy that requires a
/// Check, in a deployment that cannot make one, refuses every request that
/// reaches layer 2. With no such policy configured — the default — nothing
/// reaches it, so the synthetic trial's behaviour is unchanged.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCheckConfigured;

#[async_trait]
impl AuthorizationCheck for NoCheckConfigured {
    async fn check(
        &self,
        _credential: &CallerCredential,
        _question: &CheckQuestion,
    ) -> Result<CheckDecision, CheckFailure> {
        Err(CheckFailure::NotConfigured)
    }
}
