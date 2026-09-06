//! Comparing desired state with observed state.

use std::collections::BTreeSet;

use fabric_client_model::{Client, OidcClient, RedirectStrategy, RedirectUri};

use crate::plan::{IdentityAction, IdentityPlan};
use crate::provider::{ObservedOidcClient, ObservedRealm};

/// Works out what has to change for `observed` to match `client`.
///
/// `observed` is `None` when the realm does not exist yet, which is the one
/// case that produces a [`IdentityAction::CreateRealm`]. Everything after that
/// is the same code path whether the realm was just planned into existence or
/// has been there for a year — a new realm is simply one whose observed roles
/// and clients are empty, so there is no separate "first time" branch to drift
/// out of step with the steady-state one.
///
/// `configured_audience` is the identity provider's own deployment
/// configuration (ADR 0019 §1, §G5) — the value every declared client's
/// mapper is written to assert. It is a parameter rather than a field on
/// `client` or `observed` because it belongs to neither: it is not something
/// a desired-state document says, and it is not a fact a realm holds about
/// itself, but adapter configuration threaded into this comparison.
#[must_use]
pub fn plan(client: &Client, observed: Option<&ObservedRealm>, configured_audience: &str) -> IdentityPlan {
    let identity = &client.identity;
    let mut actions = Vec::new();

    let empty = ObservedRealm {
        display_name: client.display_name.clone(),
        roles: BTreeSet::new(),
        clients: std::collections::BTreeMap::new(),
    };

    let current = match observed {
        None => {
            actions.push(IdentityAction::CreateRealm {
                display_name: client.display_name.clone(),
            });
            &empty
        }
        Some(realm) => {
            if realm.display_name != client.display_name {
                actions.push(IdentityAction::SetRealmDisplayName {
                    display_name: client.display_name.clone(),
                });
            }
            realm
        }
    };

    for role in &identity.roles {
        if !current.roles.contains(role) {
            actions.push(IdentityAction::CreateRealmRole(role.clone()));
        }
    }

    for declared in &identity.clients {
        match current.clients.get(&declared.id) {
            None => actions.push(IdentityAction::CreateOidcClient(declared.clone())),
            Some(existing) if !matches(declared, existing, configured_audience) => {
                actions.push(IdentityAction::UpdateOidcClient(declared.clone()));
            }
            Some(_) => {}
        }
    }

    IdentityPlan::new(identity.realm.clone(), actions)
}

/// Whether an existing application client already matches its declaration.
///
/// Five terms, and every one of them is a way a client can have drifted:
///
/// - `existing.public` — a declared client is always public (see
///   [`OidcClient`] for why a confidential one cannot be expressed), so one
///   the provider now holds as confidential does not match.
/// - `existing.unmodellable_redirect_uris == 0` — a redirect URI Keycloak
///   holds that this model cannot parse is drift, not silence (ADR 0019 §6).
///   A client whose declared set is fully present *and* carries one extra,
///   unmodellable entry has still drifted from its declaration.
/// - `existing.redirect_uris == declared_uris(&declared.redirect)` — compared
///   as **sets**: the provider is free to return them in any order, and a
///   provider holding one legitimate entry beyond what is declared is drift
///   too, not merely a superset the declared set happens to fit inside.
/// - `existing.challenge_method == Some(declared.pkce)` — `challenge_method`
///   is `Option<PkceMethod>`, so an attribute Keycloak holds that this model
///   cannot read and an attribute that is simply absent both read `None`,
///   which is not `Some(S256)`. No `Plain` variant exists anywhere in this
///   model, and none is needed for a downgrade to be seen as drift.
/// - `existing.audience_mapper.as_deref() == Some(configured_audience)` — a
///   mapper that was removed by hand, or that names a different audience,
///   stops matching. Without this term the mapper would be written once and
///   could silently disappear, taking the edge's `aud` check down with it.
fn matches(declared: &OidcClient, existing: &ObservedOidcClient, configured_audience: &str) -> bool {
    existing.public
        && existing.unmodellable_redirect_uris == 0
        && existing.redirect_uris == declared_uris(&declared.redirect)
        && existing.challenge_method == Some(declared.pkce)
        && existing.audience_mapper.as_deref() == Some(configured_audience)
}

/// The redirect URIs a declaration carries, as the set [`matches`] compares
/// against what the provider reports.
fn declared_uris(redirect: &RedirectStrategy) -> BTreeSet<RedirectUri> {
    redirect.uris().iter().cloned().collect()
}
