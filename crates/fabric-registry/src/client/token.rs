//! Asking a realm the kind's rule allowed for a pull token.
//!
//! Kept in one file because what is asked of a realm and how its answer is
//! read are one exchange, only checkable side by side.
//!
//! # What a realm's refusal means
//!
//! A realm answering `401` to a credential has refused the credential: it is
//! marked refused, and not presented again (ADR 0026 section 5). A `403` is
//! read by what was asked. To a credentialed request for no scope — proving
//! the registry — it is the credential refused too, since nothing else was
//! asked. To a request for one repository it is that scope declined: GHCR's
//! realm answers `403 DENIED` for a repository it will not grant while
//! granting others to the same credential, so marking the credential then
//! would lock every other repository out over one that is gone. A declined
//! scope, and any refusal to an anonymous request, is no token: the
//! registry's own `401` stands, and is the caller's to classify.

use fabric_platform_management::RegistryError;
use reqwest::StatusCode;

use crate::client::realm::Allowed;
use crate::client::scope::Scope;
use crate::client::wire::PullToken;
use crate::client::{bounds, OciRegistry};
use crate::errors::{rate_limited, send_failure, status_failure, unreadable};
use crate::transport::{bounded_body, permits};

/// What a realm answered.
pub(super) enum Minted {
    /// A token for the scope.
    Token(String),

    /// It declined the scope, without refusing the credential.
    Declined,
}

impl OciRegistry {
    /// A token from `allowed`, for `scope`: `repository:<path>:pull`, or no
    /// scope at all when proving `/v2/` — whatever the challenge offered.
    ///
    /// The credential, when `presents` says the request is one it was
    /// registered for, goes to the realm as HTTP `Basic`.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Denied`] if the realm refused the credential, which
    /// is then marked refused; [`RegistryError::Refused`] if the realm is not
    /// HTTPS or not a public address when it must be; otherwise what the
    /// realm answered, as any status is classified, or an unreadable
    /// response past 16 KiB.
    pub(super) async fn token(
        &self,
        operation: &str,
        allowed: &Allowed,
        scope: Scope<'_>,
        presents: bool,
    ) -> Result<Minted, RegistryError> {
        let url = token_url(allowed, scope);
        if permits(self.transport, &[], &url).is_err() {
            return Err(RegistryError::Refused {
                detail: format!("{operation}: the registry's realm is not an HTTPS address"),
            });
        }
        self.address.check(&url)?;

        let mut request = self.api.get(url);
        if let (true, Some(credential)) = (presents, &self.credential) {
            request = request.basic_auth(&credential.username, Some(credential.secret.expose()));
        }

        let response = request
            .send()
            .await
            .map_err(|error| send_failure(operation, &error))?;
        let status = response.status();

        let forbidden = status == StatusCode::FORBIDDEN && !rate_limited(status, response.headers());
        if presents && (status == StatusCode::UNAUTHORIZED || (forbidden && scope == Scope::Registry)) {
            return Err(self.refuse(operation));
        }
        if status == StatusCode::UNAUTHORIZED || forbidden {
            return Ok(Minted::Declined);
        }
        if !status.is_success() {
            return Err(status_failure(operation, status, response.headers()));
        }

        let body = bounded_body(response, bounds::TOKEN, operation).await?;
        let minted: PullToken = serde_json::from_slice(&body).map_err(|_| unreadable(operation))?;
        minted
            .bearer()
            .map(Minted::Token)
            .ok_or_else(|| unreadable(operation))
    }
}

/// The token request for `scope` at `allowed`.
///
/// # Why the realm's own `scope` and `service` are dropped
///
/// A recorded or followed realm is the URL the challenge named, query and
/// all, and a registry could name `…/token?scope=repository:*:push`. A token
/// is only ever asked for `repository:<path>:pull`, or for nothing when
/// proving (ADR 0026 section 5), so every `scope` and `service` pair the
/// realm carried is dropped before this crate's own are appended.
pub(super) fn token_url(allowed: &Allowed, scope: Scope<'_>) -> reqwest::Url {
    let mut url = allowed.realm.clone();
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(name, _)| name != "scope" && name != "service")
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    url.set_query(None);
    if !kept.is_empty() {
        url.query_pairs_mut().extend_pairs(kept);
    }
    if let Some(service) = &allowed.service {
        url.query_pairs_mut().append_pair("service", service);
    }
    if let Scope::Repository(path) = scope {
        url.query_pairs_mut()
            .append_pair("scope", &format!("repository:{path}:pull"));
    }
    url
}
