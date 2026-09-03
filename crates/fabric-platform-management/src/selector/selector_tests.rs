//! Five branches, and the one that writes is the one that must be hard to
//! reach by accident.

use crate::Release;
use std::collections::BTreeMap;

use super::{decide, Decision, Reason};
use crate::{Discovery, ReleaseUnit, ResolvedImage, UpdatePolicy, Version};

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
}

/// A complete release unit, as discovery would have produced it.
fn unit(text: &str) -> ReleaseUnit {
    ReleaseUnit {
        version: version(text),
        source_revision: "5707f5e".to_owned(),
        images: BTreeMap::from([(
            "runtime".to_owned(),
            ResolvedImage {
                repository: "ghcr.io/fieldstatenz/saas-fabric".to_owned(),
                digest: "sha256:aaaa".to_owned(),
            },
        )]),
    }
}

/// Something newer exists, and it is complete.
fn something_newer() -> Discovery {
    Discovery {
        newer: Some(Release::Unit(unit("0.3.0-preview.3"))),
        ..Discovery::default()
    }
}

/// Nothing newer, but a version that is still publishing.
fn nothing_usable() -> Discovery {
    Discovery {
        newer: None,
        not_yet: vec![version("0.3.0-preview.3")],
        incoherent: Vec::new(),
    }
}

#[test]
fn a_manual_component_never_writes_however_available_the_update_is() {
    assert_eq!(
        decide(UpdatePolicy::Manual, false, &something_newer()),
        Decision::Stay(Reason::Manual)
    );
}

#[test]
fn a_locked_component_never_writes() {
    assert_eq!(
        decide(UpdatePolicy::Locked, false, &something_newer()),
        Decision::Stay(Reason::Locked)
    );
}

#[test]
fn a_hold_stops_an_automatic_component() {
    // The operator said "stay here until I tell you". They did not change the
    // policy, and this must not act as though they had.
    assert_eq!(
        decide(UpdatePolicy::Automatic, true, &something_newer()),
        Decision::Stay(Reason::Held)
    );
}

#[test]
fn an_automatic_component_with_nothing_newer_stays() {
    assert_eq!(
        decide(UpdatePolicy::Automatic, false, &nothing_usable()),
        Decision::Stay(Reason::NothingNewer)
    );
}

#[test]
fn an_automatic_component_advances_to_what_discovery_found() {
    let found = something_newer();

    let Decision::Advance(chosen) = decide(UpdatePolicy::Automatic, false, &found) else {
        panic!("an automatic component with an available update should advance");
    };

    assert_eq!(chosen, Release::Unit(unit("0.3.0-preview.3")));
}

#[test]
fn only_one_of_the_five_branches_writes() {
    // Stated as a test because it is the property that matters: every way of
    // *not* being (automatic, unheld, with something available) has to stay.
    let cases = [
        (UpdatePolicy::Manual, false),
        (UpdatePolicy::Manual, true),
        (UpdatePolicy::Locked, false),
        (UpdatePolicy::Locked, true),
        (UpdatePolicy::Automatic, true),
    ];

    for (policy, held) in cases {
        assert!(
            matches!(decide(policy, held, &something_newer()), Decision::Stay(_)),
            "{policy:?} held={held} advanced"
        );
    }

    assert!(matches!(
        decide(UpdatePolicy::Automatic, false, &something_newer()),
        Decision::Advance(_)
    ));
}

#[test]
fn a_held_component_still_reports_what_it_is_holding_back_from() {
    // Discovery keeps running while a hold stands, so the console can say
    // "Automatic — Paused" beside a newer available version. Freezing
    // availability at the held version would be less useful and less true.
    let found = something_newer();

    assert_eq!(
        decide(UpdatePolicy::Automatic, true, &found),
        Decision::Stay(Reason::Held)
    );
    assert_eq!(
        found.newer.map(|release| release.version().as_str().to_owned()),
        Some("0.3.0-preview.3".to_owned()),
        "the decision must not consume or hide what was discovered"
    );
}
