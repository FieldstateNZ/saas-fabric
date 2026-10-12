//! The recovery command accepts exactly its three forms over its two
//! targets.

use std::path::PathBuf;

use fabric_runtime_publication::DocumentKind;

use super::{parse, requested, Action, Target};

const FILES: [&str; 6] = [
    "--tenants",
    "/p/tenants.json",
    "--data-sources",
    "/p/data-sources.json",
    "--catalog",
    "/p/catalog.json",
];

fn with(action: &str, rest: &[&str]) -> Vec<String> {
    ["held-publication", action]
        .iter()
        .chain(rest)
        .map(|word| (*word).to_owned())
        .collect()
}

fn files() -> Target {
    Target::Filesystem {
        tenants: PathBuf::from("/p/tenants.json"),
        data_sources: PathBuf::from("/p/data-sources.json"),
        catalog: PathBuf::from("/p/catalog.json"),
    }
}

const RESTORE: [&str; 10] = [
    "--document",
    "data-sources",
    "--from",
    "/copy/data-sources.json",
    "--audit",
    "/ops/audit.jsonl",
    "--operator",
    "ops",
    "--reason",
    "incident 42",
];

#[test]
fn the_command_is_recognised_only_as_the_first_argument() {
    assert!(requested(["held-publication", "inspect"]));
    assert!(!requested(["/etc/fabric/control-plane.toml", "held-publication"]));
    assert!(!requested(Vec::<String>::new()));
}

#[test]
fn inspect_and_validate_take_three_paths_or_a_namespace() {
    assert_eq!(
        parse(with("inspect", &FILES)).unwrap(),
        Action::Inspect { target: files() }
    );
    assert_eq!(
        parse(with("validate", &["--namespace", "platform-system"])).unwrap(),
        Action::Validate {
            target: Target::Kubernetes {
                namespace: "platform-system".to_owned()
            }
        }
    );
}

#[test]
fn restore_takes_a_document_a_source_an_audit_log_an_operator_and_a_reason() {
    let arguments = with("restore", &[&FILES[..], &RESTORE[..]].concat());

    assert_eq!(
        parse(arguments).unwrap(),
        Action::Restore {
            target: files(),
            document: DocumentKind::DataSources,
            from: PathBuf::from("/copy/data-sources.json"),
            audit: PathBuf::from("/ops/audit.jsonl"),
            operator: "ops".to_owned(),
            reason: "incident 42".to_owned(),
        }
    );
}

#[test]
fn restore_without_every_option_or_with_a_blank_one_is_refused() {
    for missing in (0..RESTORE.len()).step_by(2) {
        let mut options = RESTORE.to_vec();
        options.drain(missing..missing + 2);
        let arguments = with("restore", &[&FILES[..], &options[..]].concat());
        assert!(parse(&arguments).is_err(), "{arguments:?}");
    }
    let mut blank = RESTORE.to_vec();
    blank[7] = " ";
    assert!(parse(with("restore", &[&FILES[..], &blank[..]].concat())).is_err());
}

#[test]
fn the_catalogue_and_any_other_document_cannot_be_restored() {
    for document in ["catalog", "catalogue", "record"] {
        let mut options = RESTORE.to_vec();
        options[1] = document;
        assert!(parse(with("restore", &[&FILES[..], &options[..]].concat())).is_err());
    }
}

#[test]
fn anything_else_is_a_usage_error() {
    let namespace = ["--namespace", "platform-system"];
    for arguments in [
        vec!["held-publication".to_owned()],
        with("inspect", &[]),
        with("inspect", &FILES[..4]),
        with("inspect", &[&FILES[..], &namespace[..]].concat()),
        with("discard", &namespace),
        with("write", &namespace),
        with("reset", &namespace),
        with("inspect", &["--namespace", "a", "--namespace", "b"]),
        with("inspect", &["--namespace"]),
        with("inspect", &["--force", "--namespace", "platform-system"]),
        with("inspect", &[&namespace[..], &["--operator", "ops"][..]].concat()),
        with("validate", &[&namespace[..], &RESTORE[..]].concat()),
    ] {
        assert!(parse(&arguments).is_err(), "{arguments:?}");
    }
}
