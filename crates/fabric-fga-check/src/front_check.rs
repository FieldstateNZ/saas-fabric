//! The HTTP adapter behind the Data API's `AuthorizationCheck` port.

use std::time::Duration;

use async_trait::async_trait;
use fabric_data_api::{AuthorizationCheck, CallerCredential, CheckDecision, CheckFailure, CheckQuestion};

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
        let relation = computed_relation(question.operation);
        let object = question.object.to_string();

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

        // Anything else that is not a 200 is the platform disagreeing with
        // itself: a 401 means the front's issuer registry refused a token the
        // runtime's ingress accepted, a 400 means this adapter sent a shape the
        // front does not take. Neither is the caller's lack of access.
        if !status.is_success() {
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
