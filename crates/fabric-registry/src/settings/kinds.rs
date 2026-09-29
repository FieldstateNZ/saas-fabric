//! One constructor per registry kind.
//!
//! Kept in one file because each kind's rules are only clear read beside
//! the others': which realm, which names, which addresses, and whether
//! `Basic` is answered.

use crate::settings::endpoint::{base_url, naming_host, origin};
use crate::settings::{AddressPolicy, RealmRule, RegistrySettings};
use crate::transport::Transport;

impl RegistrySettings {
    /// GitHub's container registry: `https://ghcr.io`, its realm
    /// `https://ghcr.io/token` with service `ghcr.io`, naming `ghcr.io/…`.
    #[must_use]
    pub fn ghcr() -> Self {
        Self::hosted("https://ghcr.io/", "ghcr.io", "https://ghcr.io/token", "ghcr.io")
    }

    /// Docker Hub: served from `https://registry-1.docker.io`, its realm
    /// `https://auth.docker.io/token` with service `registry.docker.io`,
    /// naming `docker.io/…`.
    #[must_use]
    pub fn docker_hub() -> Self {
        Self::hosted(
            "https://registry-1.docker.io/",
            "docker.io",
            "https://auth.docker.io/token",
            "registry.docker.io",
        )
    }

    /// A registry running the distribution API at `endpoint`, an HTTPS
    /// origin named by its host, read at public addresses only.
    ///
    /// `realm` is [`RealmRule::Recorded`] once it is registered, and
    /// [`RealmRule::FollowChallenge`] while it is being proven for
    /// registration, which records the origin its challenge names.
    ///
    /// # Errors
    ///
    /// A message naming the field if `endpoint` is not an HTTPS origin, is an
    /// IP literal, or `realm` is [`RealmRule::Fixed`], which only a hosted
    /// kind has.
    pub fn distribution(endpoint: &str, realm: RealmRule) -> Result<Self, String> {
        if matches!(realm, RealmRule::Fixed { .. }) {
            return Err("registry: a distribution registry's realm is recorded, never fixed".to_owned());
        }
        let endpoint = origin(endpoint, "endpoint")?;
        Ok(Self {
            naming_host: naming_host(&endpoint),
            endpoint: endpoint.into(),
            field: "endpoint",
            realm,
            honours_basic: true,
            credential: None,
            address: AddressPolicy::PublicOnly,
            transport: Transport::Https,
            loopback_is_public: false,
        })
    }

    /// The deployment's own registry, as `[platform_management.registry]`
    /// configures it: `base_url` on any network, repositories named under
    /// `naming_host`, following its own challenge.
    ///
    /// # Errors
    ///
    /// A message naming the field if `naming_host` is empty or `base_url` is
    /// not a URL, or carries a credential, a query or a fragment.
    pub fn deployment(base: &str, naming: impl Into<String>) -> Result<Self, String> {
        let naming_host = naming.into();
        if naming_host.trim().is_empty() {
            return Err("registry: registry_host is empty".to_owned());
        }
        Ok(Self {
            endpoint: base_url(base, "base_url")?.into(),
            field: "base_url",
            naming_host,
            realm: RealmRule::FollowChallenge,
            honours_basic: false,
            credential: None,
            address: AddressPolicy::Any,
            transport: Transport::Https,
            loopback_is_public: false,
        })
    }

    /// A hosted kind, whose every value is this crate's own.
    fn hosted(endpoint: &str, naming_host: &str, realm: &str, service: &str) -> Self {
        Self {
            endpoint: endpoint.to_owned(),
            field: "endpoint",
            naming_host: naming_host.to_owned(),
            realm: RealmRule::Fixed {
                realm: realm.to_owned(),
                service: service.to_owned(),
            },
            honours_basic: false,
            credential: None,
            address: AddressPolicy::PublicOnly,
            transport: Transport::Https,
            loopback_is_public: false,
        }
    }
}
