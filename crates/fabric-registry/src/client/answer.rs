//! Answering a registry's challenge, by the kind's rule and nothing else.
//!
//! # Bearer first, Basic only where it belongs
//!
//! A `Bearer` challenge is answered with a token from the realm the kind's
//! rule allows. A `Basic` one is answered only by a `distribution` registry,
//! only for a request that presents its credential, and only toward its own
//! origin — never by a hosted kind, whose credential goes to its realm
//! alone. Any other challenge, or none, is not answered, and the `401` is
//! the caller's to classify.
//!
//! # A realm is judged whole before its rule records it
//!
//! Its address and its transport are checked first, then the kind's rule.
//! A deployment following its own challenge records the first realm it is
//! allowed, so a realm it could never have asked — at a refused address, or
//! over plain HTTP — must not be the one it records and holds every later
//! challenge to.

use fabric_platform_management::RegistryError;
use reqwest::header::HeaderMap;

use crate::client::challenge::{challenges, Scheme};
use crate::client::realm::{origin, Allowed};
use crate::client::scope::{Held, Scope};
use crate::client::token::Minted;
use crate::client::OciRegistry;
use crate::transport::permits;

impl OciRegistry {
    /// What to present in answer to the challenges in `headers`, and the
    /// realm origin a `Bearer` one named — or `None` when there is nothing
    /// this client will answer, including a realm that declined the scope.
    ///
    /// What is obtained is held for the repository, so later requests
    /// present it at once.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Refused`] if the challenge named a realm the kind's
    /// rule, the address policy or the transport rule does not allow —
    /// nothing is sent to it; otherwise what asking the realm failed with.
    pub(super) async fn answer(
        &self,
        operation: &str,
        scope: Scope<'_>,
        presents: bool,
        headers: &HeaderMap,
    ) -> Result<Option<(Held, Option<String>)>, RegistryError> {
        if let Some(allowed) = self.bearer_realm(operation, headers)? {
            let Minted::Token(token) = self.token(operation, &allowed, scope, presents).await? else {
                return Ok(None);
            };
            let held = Held::Bearer(token);
            self.hold(scope, presents, &held);
            return Ok(Some((held, Some(origin(&allowed.realm)))));
        }

        let basic = challenges(headers)
            .iter()
            .any(|challenge| challenge.scheme == Scheme::Basic);
        if basic && presents && self.honours_basic {
            self.hold(scope, presents, &Held::Basic);
            return Ok(Some((Held::Basic, None)));
        }

        Ok(None)
    }

    /// The realm a `Bearer` challenge in `headers` names, once its address,
    /// its transport and the kind's rule all allow it; `None` when no
    /// `Bearer` challenge names a realm.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Refused`] if any of the three refuses it.
    pub(super) fn bearer_realm(
        &self,
        operation: &str,
        headers: &HeaderMap,
    ) -> Result<Option<Allowed>, RegistryError> {
        let offered = challenges(headers);
        let Some(bearer) = offered
            .iter()
            .find(|challenge| challenge.scheme == Scheme::Bearer)
        else {
            return Ok(None);
        };
        let Some(named) = bearer.parameter("realm") else {
            return Ok(None);
        };

        if let Ok(url) = reqwest::Url::parse(named) {
            self.address.check(&url)?;
            if permits(self.transport, &[], &url).is_err() {
                return Err(RegistryError::Refused {
                    detail: format!("{operation}: the registry's realm is not an HTTPS address"),
                });
            }
        }
        self.realm
            .allow(operation, named, bearer.parameter("service"))
            .map(Some)
    }
}
