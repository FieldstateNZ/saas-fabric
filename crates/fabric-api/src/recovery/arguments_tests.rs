//! The recovery command accepts exactly its three forms.

use std::path::Path;

use super::{parse, requested, Action};

#[test]
fn the_command_is_recognised_only_as_the_first_argument() {
    assert!(requested(["last-known-good", "inspect"]));
    assert!(!requested(["/etc/fabric/config.toml", "last-known-good"]));
    assert!(!requested(Vec::<String>::new()));
}

#[test]
fn inspect_and_validate_take_a_directory() {
    let inspect = parse(["last-known-good", "inspect", "--dir", "/var/lib/lkg"]).unwrap();
    assert!(matches!(inspect, Action::Inspect { directory } if directory == Path::new("/var/lib/lkg")));
    let validate = parse(["last-known-good", "validate", "--dir", "/var/lib/lkg"]).unwrap();
    assert!(matches!(validate, Action::Validate { .. }));
}

#[test]
fn discard_requires_an_operator_and_a_reason_both_non_blank() {
    let discard = parse([
        "last-known-good",
        "discard",
        "--dir",
        "/d",
        "--operator",
        "ops",
        "--reason",
        "incident 42",
    ])
    .unwrap();
    let Action::Discard { request, .. } = discard else {
        panic!("expected discard");
    };
    assert_eq!(
        (request.operator.as_str(), request.reason.as_str()),
        ("ops", "incident 42")
    );

    assert!(parse(["last-known-good", "discard", "--dir", "/d"]).is_err());
    assert!(parse(["last-known-good", "discard", "--dir", "/d", "--operator", "ops"]).is_err());
    assert!(parse([
        "last-known-good",
        "discard",
        "--dir",
        "/d",
        "--operator",
        " ",
        "--reason",
        "x"
    ])
    .is_err());
}

#[test]
fn anything_else_is_a_usage_error() {
    for arguments in [
        vec!["last-known-good"],
        vec!["last-known-good", "inspect"],
        vec!["last-known-good", "restore", "--dir", "/d"],
        vec!["last-known-good", "write", "--dir", "/d"],
        vec!["last-known-good", "inspect", "--dir", "/d", "--reason", "x"],
        vec!["last-known-good", "inspect", "--dir", "/d", "--dir", "/e"],
        vec!["last-known-good", "inspect", "--dir"],
        vec!["last-known-good", "inspect", "--force", "--dir", "/d"],
    ] {
        assert!(parse(&arguments).is_err(), "{arguments:?}");
    }
}
