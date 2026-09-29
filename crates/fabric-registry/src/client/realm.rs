//! Which realm a registry's tokens may be asked of: the kind's rule, held.
//!
//! # Why a challenge never decides where a credential goes
//!
//! A challenge is written by whoever answered the request. If the realm it
//! names were simply asked, a registry — or anything in front of one — could
//! send Fabric's credential to a host of its choosing. So each kind's rule
//! decides (ADR 0026 section 5): a hosted kind's realm is fixed, a
//! distribution registry's was recorded when it was registered, and the
//! deployment's is the first one it names. A challenge naming anything else
//! is refused before a byte goes to it, and the refusal names both origins —
//! an origin, never a path or a query, which is all an operator needs to
//! tell them apart.
//!
//! Kept in one file because the three rules are one decision — where a
//! credential may go — and each is only clear read beside the others.

use std::sync::Mutex;

use fabric_platform_management::RegistryError;

use crate::settings::RealmRule;

/// A realm rule, parsed, with what it has recorded.
pub(crate) enum Realm {
    /// The kind's own realm and service.
    Fixed {
        /// The token endpoint.
        realm: reqwest::Url,
        /// The service every token request names.
        service: String,
    },

    /// The origin recorded when the registry was registered, if it named one.
    Recorded(Option<reqwest::Url>),

    /// The origin the first challenge named, once one has.
    Follow(Mutex<Option<reqwest::Url>>),
}

/// Where a token request goes, and the service it names.
pub(crate) struct Allowed {
    /// The token endpoint.
    pub(crate) realm: reqwest::Url,
    /// The service, if there is one to name.
    pub(crate) service: Option<String>,
}

impl Realm {
    /// Parses a rule.
    ///
    /// # Errors
    ///
    /// A message if a fixed realm or a recorded origin is not a URL.
    pub(crate) fn from_rule(rule: &RealmRule) -> Result<Self, String> {
        let parse =
            |text: &str| reqwest::Url::parse(text).map_err(|_| "registry: the realm is not a URL".to_owned());
        Ok(match rule {
            RealmRule::Fixed { realm, service } => Self::Fixed {
                realm: parse(realm)?,
                service: service.clone(),
            },
            RealmRule::Recorded { origin } => Self::Recorded(origin.as_deref().map(parse).transpose()?),
            RealmRule::FollowChallenge => Self::Follow(Mutex::new(None)),
        })
    }

    /// The token endpoint and service a challenge naming `named` and
    /// `service` may be answered at.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Refused`] if `named` is not a URL, carries a
    /// credential, or is on an origin the rule does not allow — naming both.
    pub(crate) fn allow(
        &self,
        operation: &str,
        named: &str,
        service: Option<&str>,
    ) -> Result<Allowed, RegistryError> {
        let refused = |why: String| RegistryError::Refused {
            detail: format!("{operation}: the registry's challenge named {why}"),
        };
        let mut named =
            reqwest::Url::parse(named).map_err(|_| refused("a realm that is not a URL".to_owned()))?;
        if !named.username().is_empty() || named.password().is_some() {
            return Err(refused("a realm carrying a credential".to_owned()));
        }
        named.set_fragment(None);
        let other = |allowed: Option<&reqwest::Url>| {
            refused(format!(
                "the realm {}, where {}",
                origin(&named),
                allowed.map_or_else(
                    || "none was recorded".to_owned(),
                    |allowed| format!("only {} is allowed", origin(allowed))
                )
            ))
        };

        let recorded = match self {
            Self::Fixed { realm, service } => {
                return if same(realm, &named) {
                    Ok(Allowed {
                        realm: realm.clone(),
                        service: Some(service.clone()),
                    })
                } else {
                    Err(other(Some(realm)))
                };
            }
            Self::Recorded(recorded) => recorded.clone(),
            Self::Follow(first) => {
                let mut first = first.lock().map_err(|_| other(None))?;
                first.get_or_insert_with(|| named.clone()).clone().into()
            }
        };

        match recorded {
            Some(recorded) if same(&recorded, &named) => Ok(Allowed {
                realm: named,
                service: service.map(ToOwned::to_owned),
            }),
            recorded => Err(other(recorded.as_ref())),
        }
    }
}

/// Whether two URLs share an origin.
fn same(one: &reqwest::Url, other: &reqwest::Url) -> bool {
    one.origin() == other.origin()
}

/// A URL's origin, and nothing more of it: `https://auth.docker.io`.
pub(crate) fn origin(url: &reqwest::Url) -> String {
    url.origin().ascii_serialization()
}
