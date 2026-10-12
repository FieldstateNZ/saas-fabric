//! The HTTP adapter behind the Data API's `AuthorizationCheck` port.

use std::time::Duration;

use async_trait::async_trait;
use fabric_data_api::{
    AuthorizationCheck, CallerCredential, CheckDecision, CheckFailure, CheckQuestion, CheckTarget,
};

use crate::wire::{CheckAnswer, CheckBody};
use crate::{computed_relation, FrontEndpoint};

/// How long to wait for a decision.
///
/// The front answers from an OpenFGA in its own container, so a slow answer is
/// a wedged process; failing the request beats holding it for the whole
/// request budget.
const TIMEOUT_SECONDS: u64 = 5;

/// Asks the authorization front's runtime surface, as the caller.
pub struct FrontCheck {
    /// Where `POST /v1/check` is.
    endpoint: FrontEndpoint,

    /// The HTTP client.
    http: reqwest::Client,
}

impl FrontCheck {
    /// Builds the adapter for a front at `endpoint`.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be built.
    pub fn new(endpoint: FrontEndpoint) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECONDS))
            // A redirect would carry the caller's token to an address nobody
            // configured.
            .redirect(reqwest::redirect::Policy::none())
            // Nor through a proxy the process environment happens to name.
            .no_proxy()
            // ADR 0027 §2b: a live endpoint is HTTPS with the peer verified
            // against the host's roots, and the client refuses to send the
            // token over anything else. Certificate checks are never relaxed.
            .https_only(endpoint.requires_tls())
            .build()
            .map_err(|error| format!("authorization front client: {error}"))?;

        Ok(Self { endpoint, http })
    }
}

#[async_trait]
impl AuthorizationCheck for FrontCheck {
    async fn check(
        &self,
        credential: &CallerCredential,
        question: &CheckQuestion,
    ) -> Result<CheckDecision, CheckFailure> {
        // The model has no collection object yet (ADR 0027 §2a), so there is
        // no wire spelling for one. Refused before anything is sent, and never
        // rewritten as a record.
        let CheckTarget::Record(object) = &question.target else {
            return Err(CheckFailure::Internal);
        };

        let relation = computed_relation(question.operation);
        let object = object.to_string();

        let response = self
            .http
            .post(self.endpoint.check_url())
            .bearer_auth(credential.bearer())
            .json(&CheckBody {
                relation: &relation,
                object: &object,
            })
            .send()
            .await
            .map_err(|_| CheckFailure::Unavailable)?;

        let status = response.status();

        if status.is_server_error() {
            return Err(CheckFailure::Unavailable);
        }

        // A decision is exactly 200. Anything else is the platform disagreeing
        // with itself, and is never read as access: a 201 or 202 is not a
        // completed decision, a 401 means the front's issuer registry refused
        // a token the runtime's ingress accepted, a 400 means this adapter
        // sent a shape the front does not take.
        if status != reqwest::StatusCode::OK {
            return Err(CheckFailure::Internal);
        }

        let answer = response
            .json::<CheckAnswer>()
            .await
            .map_err(|_| CheckFailure::Internal)?;

        Ok(if answer.allowed {
            CheckDecision::Allowed
        } else {
            CheckDecision::Denied
        })
    }
}

#[cfg(test)]
mod front_check_tests;
