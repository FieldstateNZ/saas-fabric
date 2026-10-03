//! Which addresses are public, and a resolver that hands back only those.

use std::net::IpAddr;
use std::str::FromStr;

use reqwest::dns::{Name, Resolve};

use super::ranges::is_public;
use super::{refused_in, Address, NotPublic, PublicResolver};
use crate::settings::AddressPolicy;

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

/// Resolves any name with `addresses`, as the resolver would hand them on.
async fn resolved(addresses: &[&str]) -> Result<Vec<IpAddr>, String> {
    let resolver = PublicResolver::answering(addresses.iter().map(|text| ip(text)).collect());
    match resolver
        .resolve(Name::from_str("registry.example").unwrap())
        .await
    {
        Ok(found) => Ok(found.map(|address| address.ip()).collect()),
        Err(error) => Err(error.to_string()),
    }
}

#[test]
fn every_refused_range_is_refused_however_it_is_written() {
    for refused in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "100.64.0.1",
        "169.254.169.254",
        "0.0.0.0",
        "224.0.0.1",
        "255.255.255.255",
        "192.0.2.1",
        "198.51.100.1",
        "203.0.113.1",
        "198.18.0.1",
        "::",
        "::1",
        "::ffff:127.0.0.1",
        "::ffff:169.254.169.254",
        "::127.0.0.1",
        "64:ff9b::a00:1",
        "64:ff9b:1::a9fe:a9fe",
        "2002:a9fe:a9fe::1",
        "2002:7f00:1::",
        "2001:0:4136:e378:8000:63bf:3fff:fdd2",
        "fd00::1",
        "fc00::1",
        "fe80::1",
        "ff02::1",
        "2001:db8::1",
        "2001:2::1",
    ] {
        assert!(!is_public(ip(refused)), "{refused} must be refused");
    }
}

#[test]
fn a_public_address_is_public_in_either_family() {
    for public in [
        "140.82.112.33",
        "52.1.2.3",
        "::ffff:140.82.112.33",
        "2002:8c52:7021::1",
        "2606:4700::1111",
    ] {
        assert!(is_public(ip(public)), "{public} is public");
    }
}

#[tokio::test]
async fn the_resolver_refuses_a_name_with_no_public_address() {
    for refused in [
        "127.0.0.1",
        "10.0.0.1",
        "169.254.169.254",
        "::ffff:127.0.0.1",
        "fd00::1",
    ] {
        let answer = resolved(&[refused]).await;

        assert_eq!(answer, Err(super::NOT_PUBLIC.to_owned()), "{refused}");
    }
}

#[tokio::test]
async fn the_resolver_hands_back_only_the_public_addresses_it_found() {
    assert_eq!(resolved(&["140.82.112.33"]).await, Ok(vec![ip("140.82.112.33")]));
    assert_eq!(
        resolved(&["10.0.0.1", "140.82.112.33", "::1"]).await,
        Ok(vec![ip("140.82.112.33")]),
        "a private address beside a public one is never dialled"
    );
}

#[test]
fn an_ip_literal_is_refused_only_by_the_public_policy() {
    let url = |text: &str| reqwest::Url::parse(text).unwrap();
    let public_only = Address::new(AddressPolicy::PublicOnly, false);
    let any = Address::new(AddressPolicy::Any, false);

    for literal in ["https://10.0.0.1/", "https://[::1]/", "https://140.82.112.33/"] {
        assert!(public_only.refuses_literal(&url(literal)), "{literal}");
        assert!(!any.refuses_literal(&url(literal)), "{literal}");
    }
    assert!(!public_only.refuses_literal(&url("https://ghcr.io/")));
}

#[test]
fn a_refusal_is_found_however_deeply_it_is_wrapped() {
    let wrapped = std::io::Error::other(NotPublic);

    assert!(refused_in(&NotPublic));
    assert!(refused_in(&wrapped));
    assert!(!refused_in(&std::io::Error::other("elsewhere")));
}
