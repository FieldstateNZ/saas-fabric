//! Comparing desired state with observed state.
//!
//! Which client difference is product-owned drift and which is an
//! unsupported hand edit is decided in `classify`; this file only turns the
//! answer into actions, under a [`HandEditPolicy`].
//!
//! In the 121–150 line band: one function, whose rustdoc carries the
//! contract for each of its three inputs beyond the desired state — the
//! audience, the policy and the ledger baseline — beside the code that uses
//! them.

use std::collections::BTreeSet;

use fabric_client_model::Client;

use crate::ledger::AppliedBaseline;
use crate::plan::classify::compare;
use crate::plan::{HandEditPolicy, IdentityAction, IdentityPlan};
use crate::provider::{HandEdit, HandEditedField, HeldEdits, ObservedRealm};

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
///
/// `policy` decides what happens to an unsupported hand edit. Under
/// [`HandEditPolicy::Hold`] a client carrying only those is left alone, and
/// one that also has product-owned drift is written with them preserved; both
/// are listed in [`IdentityPlan::hand_edits`]. Under
/// [`HandEditPolicy::Overwrite`] every client carrying one is written whole.
///
/// `baseline` is what the declaration ledger says Fabric wrote, which decides
/// whether a missing declared callback was removed by hand, is not yet
/// written, or cannot be placed (see `classify`). A client whose missing
/// callback cannot be placed is not written at all under
/// [`HandEditPolicy::Hold`], and is reported as
/// [`HandEditedField::RedirectUrisUnconfirmed`].
#[must_use]
pub fn plan(
    client: &Client,
    observed: Option<&ObservedRealm>,
    configured_audience: &str,
    policy: HandEditPolicy,
    baseline: &AppliedBaseline,
) -> IdentityPlan {
    let identity = &client.identity;
    let mut actions = Vec::new();
    let mut hand_edits = Vec::new();

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
        let Some(existing) = current.clients.get(&declared.id) else {
            actions.push(IdentityAction::CreateOidcClient(declared.clone()));
            continue;
        };

        let comparison = compare(
            declared,
            existing,
            configured_audience,
            baseline.callbacks(&declared.id),
        );
        hand_edits.extend(comparison.held.fields().into_iter().map(|field| HandEdit {
            client: declared.id.clone(),
            field,
        }));
        if comparison.unconfirmed {
            hand_edits.push(HandEdit {
                client: declared.id.clone(),
                field: HandEditedField::RedirectUrisUnconfirmed,
            });
        }

        let (write, held) = match policy {
            HandEditPolicy::Hold => (comparison.drift && !comparison.unconfirmed, comparison.held),
            HandEditPolicy::Overwrite => (
                comparison.drift || comparison.unconfirmed || !comparison.held.is_empty(),
                HeldEdits::default(),
            ),
        };

        if write {
            actions.push(IdentityAction::UpdateOidcClient {
                client: declared.clone(),
                held,
            });
        }
    }

    IdentityPlan::new(identity.realm.clone(), actions, hand_edits)
}
