//! Comparing desired state with observed state.

use std::collections::BTreeSet;

use fabric_client_model::{Client, OidcClient};

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
#[must_use]
pub fn plan(client: &Client, observed: Option<&ObservedRealm>) -> IdentityPlan {
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
            Some(existing) if !matches(declared, existing) => {
                actions.push(IdentityAction::UpdateOidcClient(declared.clone()));
            }
            Some(_) => {}
        }
    }

    IdentityPlan::new(identity.realm.clone(), actions)
}

/// Whether an existing application client already matches its declaration.
///
/// Compares the declaration's redirect URIs as a **set**: the provider is free
/// to return them in any order, and treating a reordering as a difference
/// would make every pass rewrite the client and every client permanently
/// "drifted".
///
/// A declared client is always public — see
/// [`OidcClient`](fabric_client_model::OidcClient) for why a confidential one
/// cannot be expressed — so a client the provider holds as confidential does
/// not match, and gets corrected.
fn matches(declared: &OidcClient, existing: &ObservedOidcClient) -> bool {
    let declared_uris: BTreeSet<_> = declared.redirect.uris().iter().cloned().collect();

    existing.public && existing.redirect_uris == declared_uris
}
