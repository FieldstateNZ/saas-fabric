//! What is applied to a registry after its kind has built it: where the
//! deployment serves it, its credential, and this crate's test switches.

use crate::settings::endpoint::{base_url, loopback};
use crate::settings::{AddressPolicy, Credential, RegistrySettings};
use crate::transport::Transport;

impl RegistrySettings {
    /// The same registry, served where the deployment reads its host: at
    /// `base`, on any network the deployment chose, keeping every rule its
    /// kind has — its realm, its names, whether `Basic` is answered, its
    /// credential.
    ///
    /// # Why only the endpoint and the address policy change
    ///
    /// An operator's registry for the deployment's host contributes a
    /// credential and repositories only, and is registered at the
    /// deployment's endpoint (ADR 0026 section 5). Where it is served, and on
    /// which network, are the deployment's; where its credential may go is
    /// still its kind's — `ghcr`'s fixed realm, or the realm a
    /// `distribution` registry recorded — never whatever a challenge names
    /// after a restart.
    ///
    /// # Errors
    ///
    /// A message naming the field if `base` is not a URL, or carries a
    /// credential, a query or a fragment.
    pub fn at_deployment(mut self, base: &str) -> Result<Self, String> {
        self.endpoint = base_url(base, "base_url")?.into();
        self.field = "base_url";
        self.address = AddressPolicy::Any;
        Ok(self)
    }

    /// The same registry, holding `credential`.
    #[must_use]
    pub fn with_credential(mut self, credential: Credential) -> Self {
        self.credential = Some(credential);
        self
    }

    /// The same registry, served from `endpoint` on loopback over plain HTTP
    /// or HTTPS, keeping every rule its kind has. For this crate's tests
    /// alone, as `OciRegistry::plain_http_to_loopback` is.
    ///
    /// # Errors
    ///
    /// A message if `endpoint` is not a loopback address.
    #[doc(hidden)]
    pub fn serve_from(mut self, endpoint: &str) -> Result<Self, String> {
        self.endpoint = loopback(endpoint, self.field)?.into();
        self.transport = Transport::LoopbackToo;
        Ok(self)
    }

    /// Counts loopback as a public address. For this crate's tests alone, and
    /// honoured only once [`serve_from`](Self::serve_from) has put the
    /// registry on loopback: a production registry never sees it.
    #[doc(hidden)]
    #[must_use]
    pub fn treat_loopback_as_public(mut self) -> Self {
        self.loopback_is_public = true;
        self
    }
}
