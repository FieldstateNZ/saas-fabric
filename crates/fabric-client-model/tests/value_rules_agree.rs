//! `fabric_component`'s hostname and identifier rules give the answers
//! `Host` and `ClientId` give.
//!
//! # Why this is a test and not a shared type
//!
//! A configuration field of kind `hostname` or `identifier` was checked with
//! `Host::try_new` and `ClientId::try_new` until the field validators moved
//! to `fabric-component` (ADR 0026 section 2). That crate is in neither
//! plane and cannot name these control-plane types, so it re-declares both
//! rules over `fabric_core::naming`. Two declarations can drift; this corpus
//! is what holds them to one answer, so a value a stored catalogue's field
//! accepted is still accepted, and one it refused still is.
use fabric_client_model::{ClientId, Host};

/// Inputs chosen to sit on the edge of one rule or the other.
fn corpus() -> Vec<String> {
    let mut corpus: Vec<String> = [
        "",
        "a",
        "acme",
        "www.example.com",
        "a.b.c.d.e",
        "Acme",
        "ACME",
        "acme-",
        "-acme",
        "ac--me",
        "ac_me",
        "acme.",
        ".acme",
        "www..example.com",
        "https://acme.example",
        "acme.example:8443",
        "acme/portal",
        "acme example",
        " acme",
        "acme ",
        "*.example.com",
        "xn--bcher-kva.example",
        "1234",
        "10.0.0.1",
        "0",
        "ac.me-",
        "a-.example",
        "é.example",
        "acme\n",
        "acme\u{200B}",
        "localhost",
        "a.b-",
        "123-abc",
        "acme.example.com.",
        "UPPER.example.com",
    ]
    .iter()
    .map(|value| (*value).to_owned())
    .collect();
    corpus.push("a".repeat(63));
    corpus.push("a".repeat(64));
    corpus.push(format!("{}.example.com", "a".repeat(63)));
    corpus.push(format!("{}.example.com", "a".repeat(64)));
    let label = "a".repeat(63);
    corpus.push(
        [label.as_str(); 4]
            .join(".")
            .get(..253)
            .unwrap_or_default()
            .to_owned(),
    );
    corpus.push([label.as_str(); 4].join("."));
    corpus
}

#[test]
fn the_corpus_is_large_enough_to_mean_something() {
    assert!(corpus().len() >= 30);
}

#[test]
fn is_hostname_agrees_with_host() {
    for value in corpus() {
        assert_eq!(
            fabric_component::is_hostname(&value),
            Host::try_new(&value).is_ok(),
            "{value:?}"
        );
    }
}

#[test]
fn is_identifier_agrees_with_client_id() {
    for value in corpus() {
        assert_eq!(
            fabric_component::is_identifier(&value),
            ClientId::try_new(&value).is_ok(),
            "{value:?}"
        );
    }
}
