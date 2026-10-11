//! Every lifecycle transition, and every refusal.

use super::*;
use crate::ClientId;

const ALL_USES: [LifecycleUse; 5] = [
    LifecycleUse::Edit,
    LifecycleUse::Remove,
    LifecycleUse::Place,
    LifecycleUse::Publish,
    LifecycleUse::Route,
];

fn id(value: &str) -> ClientId {
    ClientId::try_new(value).unwrap()
}

fn all_met() -> Readiness {
    Readiness::new(Evidence::Met, Evidence::Met, Evidence::Met)
}

fn identity_pending() -> Readiness {
    Readiness::new(
        Evidence::Met,
        Evidence::Unmet("pending".to_owned()),
        Evidence::Met,
    )
}

fn tombstoned() -> LifecycleRecord {
    LifecycleRecord::default()
        .tombstone(100, "operator-1", "Contract ended")
        .unwrap()
}

fn deletion_confirmed() -> LifecycleRecord {
    tombstoned()
        .confirm_deletion(&id("acme"), &id("acme"), 200, "operator-2")
        .unwrap()
}

// Derived transitions: saved intent and ready.

#[test]
fn a_created_client_is_incomplete_until_anything_is_assessed() {
    let lifecycle = ClientLifecycle::derive(&LifecycleRecord::default(), Readiness::unassessed());

    let ClientLifecycle::Incomplete { unmet } = lifecycle else {
        panic!("expected incomplete, got {lifecycle:?}");
    };
    let conditions: Vec<_> = unmet.iter().map(|unmet| unmet.condition).collect();
    assert_eq!(
        conditions,
        [
            ReadinessCondition::Provisioning,
            ReadinessCondition::Identity,
            ReadinessCondition::Publication
        ]
    );
}

#[test]
fn incomplete_becomes_ready_once_provisioning_identity_and_publication_are_met() {
    assert_eq!(
        ClientLifecycle::derive(&LifecycleRecord::default(), all_met()),
        ClientLifecycle::Ready
    );
}

#[test]
fn each_condition_alone_keeps_a_client_incomplete() {
    let unmet = || Evidence::Unmet("no".to_owned());
    let cases = [
        (
            Readiness::new(unmet(), Evidence::Met, Evidence::Met),
            ReadinessCondition::Provisioning,
        ),
        (
            Readiness::new(Evidence::Met, unmet(), Evidence::Met),
            ReadinessCondition::Identity,
        ),
        (
            Readiness::new(Evidence::Met, Evidence::Met, unmet()),
            ReadinessCondition::Publication,
        ),
    ];

    for (readiness, condition) in cases {
        let lifecycle = ClientLifecycle::derive(&LifecycleRecord::default(), readiness);
        assert_eq!(
            lifecycle,
            ClientLifecycle::Incomplete {
                unmet: vec![Unmet {
                    condition,
                    detail: "no".to_owned()
                }]
            }
        );
    }
}

#[test]
fn ready_falls_back_to_incomplete_when_evidence_regresses() {
    let record = LifecycleRecord::default();
    assert_eq!(
        ClientLifecycle::derive(&record, all_met()).state(),
        LifecycleState::Ready
    );

    assert_eq!(
        ClientLifecycle::derive(&record, identity_pending()).state(),
        LifecycleState::Incomplete
    );
}

// Recorded transitions: tombstone, then confirmed deletion.

#[test]
fn an_incomplete_client_can_be_tombstoned() {
    let record = LifecycleRecord::default()
        .tombstone(100, "operator-1", "  Created by mistake  ")
        .unwrap();

    assert_eq!(
        ClientLifecycle::derive(&record, identity_pending()),
        ClientLifecycle::Tombstoned {
            tombstone: Tombstone {
                at: 100,
                by: "operator-1".to_owned(),
                reason: "Created by mistake".to_owned(),
            }
        }
    );
}

#[test]
fn a_ready_client_can_be_tombstoned_and_evidence_does_not_make_it_ready_again() {
    assert_eq!(
        ClientLifecycle::derive(&tombstoned(), all_met()).state(),
        LifecycleState::Tombstoned
    );
}

#[test]
fn tombstoning_never_confirms_deletion() {
    assert_eq!(tombstoned().deletion_confirmed, None);
}

#[test]
fn a_tombstoned_client_can_have_its_deletion_confirmed() {
    let record = deletion_confirmed();

    assert_eq!(
        ClientLifecycle::derive(&record, all_met()),
        ClientLifecycle::DeletionConfirmed {
            tombstone: tombstoned().tombstone.unwrap(),
            deletion: DeletionConfirmation {
                at: 200,
                by: "operator-2".to_owned(),
            },
        }
    );
}

// Transition refusals.

#[test]
fn deletion_cannot_be_confirmed_without_a_tombstone() {
    assert_eq!(
        LifecycleRecord::default().confirm_deletion(&id("acme"), &id("acme"), 200, "o"),
        Err(LifecycleRefusal::DeletionRequiresTombstone)
    );
}

#[test]
fn deletion_cannot_be_confirmed_twice() {
    assert_eq!(
        deletion_confirmed().confirm_deletion(&id("acme"), &id("acme"), 300, "o"),
        Err(LifecycleRefusal::DeletionAlreadyConfirmed)
    );
}

#[test]
fn a_confirmation_naming_another_client_is_refused() {
    assert_eq!(
        tombstoned().confirm_deletion(&id("acme"), &id("acne"), 200, "o"),
        Err(LifecycleRefusal::ConfirmationMismatch)
    );
}

#[test]
fn a_tombstoned_client_cannot_be_tombstoned_again() {
    assert_eq!(
        tombstoned().tombstone(300, "o", "again"),
        Err(LifecycleRefusal::AlreadyTombstoned)
    );
}

#[test]
fn a_client_whose_deletion_is_confirmed_cannot_be_tombstoned() {
    assert_eq!(
        deletion_confirmed().tombstone(300, "o", "again"),
        Err(LifecycleRefusal::DeletionAlreadyConfirmed)
    );
}

#[test]
fn a_tombstone_needs_an_acceptable_reason() {
    for reason in ["", "   ", "line\nbreak", &"x".repeat(513)] {
        assert!(
            matches!(
                LifecycleRecord::default().tombstone(100, "o", reason),
                Err(LifecycleRefusal::InvalidReason { .. })
            ),
            "{reason:?} was accepted"
        );
    }
    assert!(LifecycleRecord::default()
        .tombstone(100, "o", &"x".repeat(512))
        .is_ok());
}

// What each state permits.

#[test]
fn a_ready_client_permits_every_use() {
    let lifecycle = ClientLifecycle::derive(&LifecycleRecord::default(), all_met());

    for operation in ALL_USES {
        assert_eq!(lifecycle.permits(operation), Ok(()));
    }
}

#[test]
fn an_incomplete_client_cannot_be_routed() {
    let lifecycle = ClientLifecycle::derive(&LifecycleRecord::default(), identity_pending());

    assert_eq!(
        lifecycle.permits(LifecycleUse::Route),
        Err(LifecycleRefusal::NotReady {
            operation: LifecycleUse::Route,
            unmet: vec![ReadinessCondition::Identity],
        })
    );
}

#[test]
fn an_incomplete_client_can_still_be_edited_placed_and_published_towards_readiness() {
    let lifecycle = ClientLifecycle::derive(&LifecycleRecord::default(), Readiness::unassessed());

    for operation in [
        LifecycleUse::Edit,
        LifecycleUse::Remove,
        LifecycleUse::Place,
        LifecycleUse::Publish,
    ] {
        assert_eq!(lifecycle.permits(operation), Ok(()));
    }
}

#[test]
fn a_tombstoned_client_refuses_every_use() {
    let lifecycle = ClientLifecycle::derive(&tombstoned(), all_met());

    for operation in ALL_USES {
        assert_eq!(
            lifecycle.permits(operation),
            Err(LifecycleRefusal::Retired {
                state: LifecycleState::Tombstoned,
                operation,
            })
        );
    }
}

#[test]
fn a_client_whose_deletion_is_confirmed_refuses_every_use() {
    let lifecycle = ClientLifecycle::derive(&deletion_confirmed(), all_met());

    for operation in ALL_USES {
        assert_eq!(
            lifecycle.permits(operation),
            Err(LifecycleRefusal::Retired {
                state: LifecycleState::DeletionConfirmed,
                operation,
            })
        );
    }
}

#[test]
fn a_refusal_says_what_was_refused_and_that_state_is_retained() {
    let refusal = LifecycleRefusal::Retired {
        state: LifecycleState::Tombstoned,
        operation: LifecycleUse::Publish,
    };

    assert_eq!(
        refusal.to_string(),
        "the client is tombstoned, so publication is refused; its state is retained"
    );
}
