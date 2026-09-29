//! Proving a registry before it is recorded (ADR 0026 section 5).
//!
//! # What a proof says, and what it does not
//!
//! That the registry's `/v2/` endpoint answered through its challenge — with
//! the credential, when one is held — and which realm origin that challenge
//! named. Never "connected": what the console shows is what was proven.
//!
//! # Why an anonymous proof ends at the challenge
//!
//! With no credential there is nothing to present, and a token for no scope
//! proves nothing a challenge did not: GHCR's realm refuses an anonymous
//! request for one outright, while granting every public repository. So a
//! `Bearer` challenge naming a realm the kind's rule, the address policy and
//! the transport rule all allow *is* the anonymous proof, and nothing is sent
//! to the realm. With a credential the realm is asked, for no scope, so the
//! credential itself is proven.

use fabric_platform_management::RegistryError;
use reqwest::{Method, StatusCode};

use crate::client::realm::origin;
use crate::client::scope::Scope;
use crate::client::send::{Call, Via};
use crate::client::OciRegistry;
use crate::errors::status_failure;

/// What proving a registry found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proof {
    /// The origin of the realm its challenge named.
    realm: Option<String>,
}

impl Proof {
    /// The origin of the realm the registry's `Bearer` challenge named —
    /// `https://auth.docker.io` — or `None` when it asked for `Basic`, or
    /// for nothing. What a `distribution` registry records when it is
    /// registered.
    #[must_use]
    pub fn realm_origin(&self) -> Option<&str> {
        self.realm.as_deref()
    }
}

impl OciRegistry {
    /// Proves the registry: `GET /v2/` through its challenge, with the
    /// credential when one is held.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Denied`] if the realm refused the credential, or had
    /// already; [`RegistryError::Refused`] if the challenge named a realm the
    /// kind does not allow, or the endpoint answered anything but success;
    /// [`RegistryError::Unavailable`] if it could not be asked.
    pub async fn prove(&self) -> Result<Proof, RegistryError> {
        let operation = "proving the registry";
        let url = format!("{}/v2/", self.base_url);
        let call = Call {
            method: Method::GET,
            via: Via::Api,
            operation,
            url: &url,
            accept: "application/json",
        };

        if self.credential.is_none() {
            let response = self.attempt(&call, None).await?;
            let status = response.status();
            if status.is_success() {
                return Ok(Proof { realm: None });
            }
            if status == StatusCode::UNAUTHORIZED {
                if let Some(allowed) = self.bearer_realm(operation, response.headers())? {
                    return Ok(Proof {
                        realm: Some(origin(&allowed.realm)),
                    });
                }
            }
            return Err(status_failure(operation, status, response.headers()));
        }

        let exchange = self.exchange(&call, Scope::Registry).await?;
        let status = exchange.response.status();
        if !status.is_success() {
            return Err(status_failure(operation, status, exchange.response.headers()));
        }

        Ok(Proof {
            realm: exchange.realm,
        })
    }
}
