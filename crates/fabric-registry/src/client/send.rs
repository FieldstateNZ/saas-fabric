//! Sending a request: anonymously, or with what a challenge asked for.
//!
//! Kept in one file because the first attempt, the one challenge answered
//! and the retry are one exchange.
//!
//! # Nothing is sent before a challenge asks for it
//!
//! A request goes first with whatever is already held for its repository —
//! nothing, on the first — and a registry that never challenges is read with
//! no credential at all. A `401` carrying a challenge is answered once, by
//! the kind's rule, and the request sent again; a token that aged out, or was
//! revoked early, takes the same path, so there is one mechanism instead of
//! two. A second `401` is the caller's to classify: a refusal after a token
//! was issued, never an answer.

use reqwest::header::ACCEPT;
use reqwest::{Method, Response, StatusCode};

pub(super) use crate::client::call::{Call, Exchange, Via};

use fabric_platform_management::RegistryError;

use crate::client::scope::{Held, Scope};
use crate::client::OciRegistry;
use crate::errors::send_failure;

impl OciRegistry {
    /// `GET` through the API client.
    pub(super) async fn get(
        &self,
        operation: &str,
        repository: &str,
        url: &str,
        accept: &str,
    ) -> Result<Response, RegistryError> {
        self.send(Method::GET, Via::Api, operation, repository, url, accept)
            .await
    }

    /// Sends a request for one repository.
    pub(super) async fn send(
        &self,
        method: Method,
        via: Via,
        operation: &str,
        repository: &str,
        url: &str,
        accept: &str,
    ) -> Result<Response, RegistryError> {
        let call = Call {
            method,
            via,
            operation,
            url,
            accept,
        };
        let scope = Scope::Repository(self.path(repository));
        Ok(self.exchange(&call, scope).await?.response)
    }

    /// Sends `call`, answering one challenge if it is met with one.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Denied`] without contacting anything when the
    /// request would present a credential its realm refused; otherwise what
    /// sending, or answering the challenge, failed with.
    pub(super) async fn exchange(
        &self,
        call: &Call<'_>,
        scope: Scope<'_>,
    ) -> Result<Exchange, RegistryError> {
        let presents = self.presents(scope);
        if presents {
            self.not_refused(call.operation)?;
        }

        let response = self.attempt(call, self.held(scope, presents).as_ref()).await?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(Exchange {
                response,
                realm: None,
            });
        }

        let Some((held, realm)) = self
            .answer(call.operation, scope, presents, response.headers())
            .await?
        else {
            return Ok(Exchange {
                response,
                realm: None,
            });
        };
        let response = self.attempt(call, Some(&held)).await?;

        // `Basic` is the credential itself, sent to the registry: a `401` to
        // it is the credential refused, as a realm's would be.
        if matches!(held, Held::Basic) && response.status() == StatusCode::UNAUTHORIZED {
            return Err(self.refuse(call.operation));
        }
        Ok(Exchange { response, realm })
    }

    /// Sends one request, with `held` attached.
    pub(super) async fn attempt(
        &self,
        call: &Call<'_>,
        held: Option<&Held>,
    ) -> Result<Response, RegistryError> {
        let client = match call.via {
            Via::Api => &self.api,
            Via::Blob => &self.blobs,
        };
        let request = client
            .request(call.method.clone(), call.url)
            .header(ACCEPT, call.accept);

        self.authorize(request, held)
            .send()
            .await
            .map_err(|error| send_failure(call.operation, &error))
    }
}
