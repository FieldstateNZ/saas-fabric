//! The first set a registry ever sees, and what it is allowed to leave behind.
//!
//! Applying primes the registry as a side effect and nothing can un-prime it,
//! so a first load is the one apply that must be able to *not happen*.
//!
//! The invariant these pin down: **nothing decides whether a set is usable
//! except the code that decides what to install.** It used to be two functions
//! — a predicate asking "does any entry validate?" ahead of a merge asking "is
//! this the first entry for its key, and does it validate?" — and they
//! disagreed on `[invalid a@1, valid a@2]`. The predicate waved it through, the
//! merge installed nothing, and the registry primed empty: `/ready` answered
//! 200 over zero tenants while every request failed.
//!
//! So the tests below never assert the two agree. They assert there is only one
//! answer: the outcome of `apply_all` and the state of the registry afterwards
//! are produced by the same merge and cannot come apart.
//!
//! There is likewise no longer a separate `apply_first_load` to call. That was
//! the second version of the same mistake — one rule, two call sites obliged to
//! agree about it — and the background refresh loop kept calling `apply_all`,
//! undoing a correctly-refused prime one interval later. `apply_all` now decides
//! for itself whether it is a first load, so the tests here and the ones in
//! `registration_tests` are exercising the same method the refresher does.

use fabric_core::BindingRevision;

use crate::resource::registry::test_resource::{invalid_resource, registry, resource, TestResource};

#[test]
fn the_verdict_and_the_installed_state_can_never_disagree() {
    // Every shape a first load can take, including the four the old split got
    // wrong. Nothing here asserts a specific outcome per case on purpose: the
    // property is that whatever the merge decided is what the registry shows.
    let cases: Vec<Vec<TestResource>> = vec![
        vec![],
        vec![resource("a", 1)],
        vec![invalid_resource("a", 1)],
        vec![invalid_resource("a", 1), invalid_resource("b", 1)],
        vec![invalid_resource("a", 1), resource("b", 1)],
        vec![invalid_resource("a", 1), resource("a", 2)],
        vec![resource("a", 2), invalid_resource("a", 1)],
        vec![invalid_resource("a", 1), invalid_resource("a", 2)],
        vec![
            invalid_resource("a", 1),
            invalid_resource("a", 2),
            resource("a", 3),
        ],
        vec![resource("a", 1), resource("a", 2)],
    ];

    for incoming in cases {
        let published = incoming.len();
        let registry = registry();

        let accepted = registry.apply_all(incoming).is_ok();

        assert_eq!(
            accepted,
            registry.is_primed(),
            "an accepted load must prime and a refused one must leave the registry untouched"
        );
        assert!(
            !(registry.is_primed() && registry.is_empty() && published > 0),
            "primed and empty over a publication of {published}: /ready answers 200 while \
             every request that touches the registry fails"
        );
    }
}

#[test]
fn a_load_whose_first_entry_for_a_key_is_invalid_still_serves_the_valid_one() {
    // The reviewer's direct probe, at the seam where the two answers used to be
    // computed. A valid entry for `a` was published, so a load that installs
    // nothing is not an option.
    let registry = registry();

    let report = registry
        .apply_all(vec![invalid_resource("a", 1), resource("a", 2)])
        .unwrap();

    assert_eq!(report.added, 1);
    assert_eq!(report.invalid_rejected, 1);
    assert_eq!(report.duplicate_rejected, 0);
    assert!(registry.is_primed());
    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap().revision,
        BindingRevision::new(2)
    );
}

#[test]
fn an_empty_publication_still_primes() {
    // A deployment that has not onboarded a tenant yet must start. Installing
    // nothing is only a failure when something was offered to install.
    let registry = registry();

    assert!(registry.apply_all(Vec::new()).is_ok());
    assert!(registry.is_primed());
    assert!(registry.is_empty());
}

#[test]
fn a_wholly_unusable_publication_is_refused_and_names_what_to_fix() {
    let registry = registry();

    let refused = registry
        .apply_all(vec![invalid_resource("a", 1), invalid_resource("b", 1)])
        .unwrap_err();

    assert!(!registry.is_primed());
    assert_eq!(refused.published, 2);
    assert!(
        refused.reason.starts_with("test resource a:"),
        "the refusal must name the first rejection so the log says what to go and fix, got {}",
        refused.reason
    );
}

#[test]
fn a_refused_first_load_is_refused_again_every_time_it_is_reoffered() {
    // The refresh loop's shape, with the timer taken out of it. The source does
    // not have to change: it keeps publishing the same unusable set, and every
    // apply after the refused prime is offered exactly what was refused.
    //
    // While the rule lived at the call sites, the second of these calls was the
    // bug — the loop called the method that installed unconditionally, so one
    // refresh interval after startup the registry primed over an empty snapshot
    // and `/ready` flipped 503 → 200. The registry now answers for itself, so
    // there is no second call site to disagree.
    let registry = registry();

    for attempt in 1..=3 {
        assert!(
            registry.apply_all(vec![invalid_resource("a", 1)]).is_err(),
            "attempt {attempt} installed a set that attempt 1 correctly refused"
        );
        assert!(
            !registry.is_primed(),
            "attempt {attempt} primed an empty registry"
        );
    }
}

#[test]
fn once_primed_a_full_sync_may_still_empty_the_registry() {
    // The refusal is deliberately scoped to a registry that has never loaded,
    // and must stay that way: absence from the incoming set is how
    // deprovisioning is expressed, so a registry that is already serving has to
    // be allowed to go to zero. Priming is the irreversible step, and by here it
    // has already happened — `is_primed` stays honest either way.
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]).unwrap();

    let report = registry.apply_all(vec![invalid_resource("b", 1)]).unwrap();

    assert_eq!(report.invalid_rejected, 1);
    assert_eq!(report.removed, 1);
    assert!(registry.is_primed());
    assert!(registry.is_empty());
}

#[test]
fn a_refused_load_announces_nothing() {
    // Events are published only after the swap. A refused load performs no
    // swap, so a subscriber must not be told to re-read a resource the registry
    // does not hold.
    let registry = registry();
    let mut changes = registry.subscribe();

    assert!(registry.apply_all(vec![invalid_resource("a", 1)]).is_err());

    assert!(changes.try_recv().is_err());
}

#[test]
fn one_rejection_among_many_is_not_a_load_failure() {
    // Refusing here would take every healthy tenant offline over one operator's
    // typo — worse than the fault, and at prime the blast radius is the whole
    // replica rather than one resource.
    let registry = registry();

    let report = registry
        .apply_all(vec![resource("a", 1), invalid_resource("b", 1), resource("c", 1)])
        .unwrap();

    assert_eq!(report.added, 2);
    assert_eq!(report.invalid_rejected, 1);
    assert_eq!(registry.len(), 2);
}
