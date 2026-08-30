//! `Check`: may the authenticated caller do this to this object?
//!
//! # Fabric's request shape, not the authorization service's
//!
//! ```json
//! { "relation": "viewer", "object": "document:123" }
//! ```
//!
//! There is no `user` field to overwrite, no `store_id` to ignore, and no
//! tenant or realm to strip, because none of them is in the schema. That makes
//! the security property **structural** rather than procedural: an operation
//! that accepted the service's own `CheckRequest` and sanitised it would be one
//! forgotten field away from letting a caller ask about somebody else.
//!
//! `deny_unknown_fields` is part of that. A request carrying `user` is
//! *refused*, not quietly ignored — a caller trying to name their own
//! principal gets an error rather than a decision that silently ignored them.
//!
//! # If a caller must ever ask about another subject
//!
//! That is a different operation with its own contract and its own
//! authorization — "may this operator administer access for that subject?" —
//! and never an optional `user` field added here (ADR 0016).

#[cfg(test)]
mod check_tests;

use std::sync::Arc;

use async_trait::async_trait;
use fabric_core::RelationName;
use serde::Deserialize;

use crate::{ObjectRef, VerifiedIdentity};

/// The type every principal is written under in the authorization model.
const USER_TYPE: &str = "user";

/// What a caller may ask on the runtime surface.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRequest {
    /// What the caller is asking to be to the object.
    pub relation: RelationName,

    /// The object in question.
    pub object: ObjectRef,
}

/// Why a decision could not be reached.
///
/// One variant, deliberately: reaching the authorization service is the only
/// thing that can go wrong here that is not already a refused credential or a
/// malformed request. It answers `503`, never `401` — the caller's token was
/// fine.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecisionError {
    /// The authorization service could not be reached or did not answer.
    #[error("the authorization service is unavailable")]
    Unavailable,
}

/// The embedded authorization service, as this crate uses it.
///
/// A port, so the operation's binding rules can be tested without one running
/// — and so the only code that speaks the service's protocol is the one
/// adapter behind it.
#[async_trait]
pub trait Decisions: Send + Sync {
    /// Asks whether `user` holds `relation` on `object` in `store`.
    ///
    /// # Errors
    ///
    /// Returns a message when no decision could be obtained. Every failure
    /// here is an availability failure; this port cannot refuse a credential.
    async fn check(&self, store: &str, user: &str, relation: &str, object: &str) -> Result<bool, String>;
}

/// The runtime surface's `Check` operation.
pub struct Check {
    /// Where decisions are obtained.
    decisions: Arc<dyn Decisions>,
}

impl Check {
    /// Builds the operation over an authorization service.
    #[must_use]
    pub fn new(decisions: Arc<dyn Decisions>) -> Self {
        Self { decisions }
    }

    /// Answers the request, about the authenticated caller and nobody else.
    ///
    /// Three inputs, and the caller supplies one and a half of them: the
    /// relation and the object come from the request; the principal and the
    /// store come from the verified identity, which came from the registry.
    ///
    /// # Errors
    ///
    /// [`DecisionError::Unavailable`] when no decision could be obtained. A
    /// caller who is simply not permitted gets `Ok(false)` — that is an
    /// answer, not a failure.
    pub async fn run(
        &self,
        identity: &VerifiedIdentity,
        request: &CheckRequest,
    ) -> Result<bool, DecisionError> {
        // The subject is the authenticated principal. Not a parameter, not a
        // default, not overridable: there is nowhere for a caller to say
        // otherwise.
        let user = format!("{USER_TYPE}:{}", identity.principal());

        self.decisions
            .check(
                identity.store(),
                &user,
                request.relation.as_str(),
                &request.object.to_string(),
            )
            .await
            .map_err(|_| DecisionError::Unavailable)
    }
}
