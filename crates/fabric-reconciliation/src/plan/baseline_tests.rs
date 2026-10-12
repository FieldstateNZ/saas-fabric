//! D01-15: a declared callback the provider lacks is classified against what
//! the ledger says Fabric wrote — removed by hand, not yet written, or not
//! placeable — so a legitimate host change is written and a hand removal is
//! still held.

use fabric_client_model::{OidcClient, RedirectStrategy, RedirectStrategyKind, RedirectUri};

use super::diff_tests::{converged_realm, hand_edit, AUDIENCE};
use crate::fixtures::{acme, web_client};
use crate::ledger::{callback_digests, AppliedBaseline, CallbackDigest, WriteStage};
use crate::plan::{plan, HandEditPolicy, IdentityAction};
use crate::provider::{HandEditedField, HeldEdits, ObservedRealm};

const OLD_TEMPLATE: &str = "https://www.example.com/callback";
const OLD_HOST: &str = "https://www.example.com/silent";
const NEW_HOST: &str = "https://portal.example.com/web/callback";
const INTERNAL: &str = "https://acme.lucentroot.internal/web/callback";

fn uri(value: &str) -> RedirectUri {
    RedirectUri::try_new(value).unwrap()
}

/// `web`, declared with these callbacks under this strategy.
fn web(kind: RedirectStrategyKind, uris: &[&str]) -> OidcClient {
    OidcClient {
        redirect: RedirectStrategy::try_new(kind, uris.iter().map(|value| uri(value)).collect()).unwrap(),
        ..web_client()
    }
}

fn acme_declaring(declared: &OidcClient) -> fabric_client_model::Client {
    let mut client = acme();
    client.identity.clients = vec![declared.clone()];
    client
}

/// A realm in which `web` holds exactly `uris`.
fn holding(uris: &[&str]) -> ObservedRealm {
    let mut realm = converged_realm();
    realm.clients.get_mut(&web_client().id).unwrap().redirect_uris =
        uris.iter().map(|value| uri(value)).collect();
    realm
}

fn confirmed(uris: &[&str]) -> AppliedBaseline {
    let mut baseline = AppliedBaseline::unknown();
    baseline.replay(
        web_client().id,
        uris.iter().map(|value| CallbackDigest::of(&uri(value))).collect(),
        WriteStage::Written,
    );
    baseline
}

fn held(removed: &[&str]) -> HeldEdits {
    HeldEdits {
        removed_redirect_uris: removed.iter().map(|value| uri(value)).collect(),
        ..HeldEdits::default()
    }
}

#[test]
fn a_public_to_public_host_change_writes_the_new_callback_and_reports_nothing() {
    let declared = web(RedirectStrategyKind::ClaimedHttps, &[OLD_TEMPLATE, NEW_HOST]);

    let plan = plan(
        &acme_declaring(&declared),
        Some(&holding(&[OLD_TEMPLATE, OLD_HOST])),
        AUDIENCE,
        HandEditPolicy::Hold,
        &confirmed(&[OLD_TEMPLATE, OLD_HOST]),
    );

    assert_eq!(
        plan.actions(),
        [IdentityAction::UpdateOidcClient {
            client: declared,
            held: HeldEdits::default()
        }],
        "the new host is written whole and the old one, now undeclared, goes"
    );
    assert!(plan.hand_edits().is_empty(), "no false handEdited");
}

#[test]
fn a_public_to_internal_change_replaces_every_callback() {
    let declared = web(RedirectStrategyKind::PrivateNetwork, &[INTERNAL]);

    let plan = plan(
        &acme_declaring(&declared),
        Some(&holding(&[OLD_TEMPLATE, OLD_HOST])),
        AUDIENCE,
        HandEditPolicy::Hold,
        &confirmed(&[OLD_TEMPLATE, OLD_HOST]),
    );

    assert_eq!(
        plan.actions(),
        [IdentityAction::UpdateOidcClient {
            client: declared,
            held: HeldEdits::default()
        }]
    );
    assert!(plan.hand_edits().is_empty());
}

#[test]
fn a_callback_written_before_and_removed_by_hand_is_still_held_beside_a_transition() {
    let declared = web(RedirectStrategyKind::ClaimedHttps, &[OLD_TEMPLATE, NEW_HOST]);

    let plan = plan(
        &acme_declaring(&declared),
        Some(&holding(&[OLD_HOST])),
        AUDIENCE,
        HandEditPolicy::Hold,
        &confirmed(&[OLD_TEMPLATE, OLD_HOST]),
    );

    assert_eq!(
        plan.actions(),
        [IdentityAction::UpdateOidcClient {
            client: declared,
            held: held(&[OLD_TEMPLATE]),
        }],
        "the new callback is written and the removed one stays removed"
    );
    assert_eq!(
        plan.hand_edits(),
        [hand_edit(HandEditedField::DeclaredRedirectUriRemoved)]
    );
}

#[test]
fn a_disabled_client_stays_disabled_through_a_transition() {
    let declared = web(RedirectStrategyKind::ClaimedHttps, &[OLD_TEMPLATE, NEW_HOST]);
    let mut realm = holding(&[OLD_TEMPLATE, OLD_HOST]);
    realm.clients.get_mut(&web_client().id).unwrap().enabled = false;

    let plan = plan(
        &acme_declaring(&declared),
        Some(&realm),
        AUDIENCE,
        HandEditPolicy::Hold,
        &confirmed(&[OLD_TEMPLATE, OLD_HOST]),
    );

    assert_eq!(
        plan.actions(),
        [IdentityAction::UpdateOidcClient {
            client: declared,
            held: HeldEdits {
                client_disabled: true,
                ..HeldEdits::default()
            },
        }]
    );
    assert_eq!(plan.hand_edits(), [hand_edit(HandEditedField::ClientDisabled)]);
}

#[test]
fn with_no_baseline_a_missing_callback_holds_the_whole_client_before_anything_is_removed() {
    let declared = web(RedirectStrategyKind::ClaimedHttps, &[OLD_TEMPLATE, NEW_HOST]);
    let mut realm = holding(&[OLD_TEMPLATE, OLD_HOST]);
    realm.clients.get_mut(&web_client().id).unwrap().audience_mapper = None;

    let plan = plan(
        &acme_declaring(&declared),
        Some(&realm),
        AUDIENCE,
        HandEditPolicy::Hold,
        &AppliedBaseline::unknown(),
    );

    assert!(
        plan.is_converged(),
        "no write, so the old callback is not removed: {plan:?}"
    );
    assert_eq!(
        plan.hand_edits(),
        [hand_edit(HandEditedField::RedirectUrisUnconfirmed)]
    );
}

#[test]
fn a_callback_only_recorded_as_being_written_is_not_placeable_either() {
    let declared = web(RedirectStrategyKind::ClaimedHttps, &[OLD_TEMPLATE, NEW_HOST]);
    let mut baseline = confirmed(&[OLD_TEMPLATE, OLD_HOST]);
    baseline.replay(web_client().id, callback_digests(&declared), WriteStage::Writing);

    let plan = plan(
        &acme_declaring(&declared),
        Some(&holding(&[OLD_TEMPLATE, OLD_HOST])),
        AUDIENCE,
        HandEditPolicy::Hold,
        &baseline,
    );

    assert!(plan.is_converged());
    assert_eq!(
        plan.hand_edits(),
        [hand_edit(HandEditedField::RedirectUrisUnconfirmed)]
    );
}

#[test]
fn with_no_baseline_a_converged_client_reports_nothing() {
    let plan = plan(
        &acme(),
        Some(&converged_realm()),
        AUDIENCE,
        HandEditPolicy::Hold,
        &AppliedBaseline::unknown(),
    );

    assert!(plan.is_converged());
    assert!(plan.hand_edits().is_empty());
}

#[test]
fn a_reassertion_writes_an_unplaceable_client_whole() {
    let declared = web(RedirectStrategyKind::ClaimedHttps, &[OLD_TEMPLATE, NEW_HOST]);

    let plan = plan(
        &acme_declaring(&declared),
        Some(&holding(&[OLD_TEMPLATE, OLD_HOST])),
        AUDIENCE,
        HandEditPolicy::Overwrite,
        &AppliedBaseline::unknown(),
    );

    assert_eq!(
        plan.actions(),
        [IdentityAction::UpdateOidcClient {
            client: declared,
            held: HeldEdits::default()
        }]
    );
}

#[test]
fn whether_a_write_must_be_recorded_follows_the_confirmed_set() {
    let declared = web_client();
    let written: Vec<&str> = declared.redirect.uris().iter().map(RedirectUri::as_str).collect();

    assert!(AppliedBaseline::unknown().must_record(&declared));
    assert!(!confirmed(&written).must_record(&declared));
    assert!(confirmed(&[OLD_TEMPLATE]).must_record(&declared));
    let mut pending = confirmed(&written);
    pending.replay(
        declared.id.clone(),
        callback_digests(&declared),
        WriteStage::Writing,
    );
    assert!(pending.must_record(&declared));
}
