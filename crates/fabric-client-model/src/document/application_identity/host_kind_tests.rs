//! D01-15: a projected client follows the kind of the client's declared
//! hosts, and the stored-projection check (D01-19b) follows it too.
//!
//! Over the client fixture, edited the way a Git commit would edit it.

use crate::{
    ClientDocument, DesiredStateError, OidcClient, OidcClientId, ProjectedClientDivergence,
    ProjectedClientField, ProjectionCheck, RedirectStrategyKind, RedirectUri,
};

const FIXTURE: &str = include_str!("../../../tests/fixtures/client.yaml");
const PUBLIC_HOSTS: &str = "  hosts:\n  - acme.example.com\n";
const PUBLIC_REDIRECT: &str = "        strategy: claimedHttps\n        uris:\n        - https://acme.reports.example.com/callback\n        - https://acme.example.com/reports/callback\n";
const INTERNAL_HOSTS: &str = "  hosts:\n  - acme.lucentroot.internal\n  - reports.acme.internal\n";
const INTERNAL_REDIRECT: &str = "        strategy: privateNetwork\n        uris:\n        - http://acme.lucentroot.internal/reports/callback\n        - http://reports.acme.internal/reports/callback\n";

fn edited(edit: impl FnOnce(String) -> String) -> ClientDocument {
    ClientDocument::parse(&edit(FIXTURE.to_owned())).expect("the edited text must still parse")
}

fn internal(redirect: &str) -> ClientDocument {
    edited(|text| {
        text.replace(PUBLIC_HOSTS, INTERNAL_HOSTS)
            .replace(PUBLIC_REDIRECT, redirect)
    })
}

fn reports() -> OidcClientId {
    OidcClientId::try_new("reports").unwrap()
}

fn only(projected: &[OidcClient]) -> (&RedirectStrategyKind, Vec<&str>) {
    let [client] = projected else {
        panic!("expected exactly one projected client, got {projected:?}");
    };
    (
        client.redirect.kind(),
        client.redirect.uris().iter().map(RedirectUri::as_str).collect(),
    )
}

#[test]
fn a_client_on_public_hosts_is_projected_exactly_as_before() {
    let document = ClientDocument::parse(FIXTURE).unwrap();
    let projected = document.projected_clients().unwrap();

    assert_eq!(
        only(&projected),
        (
            &RedirectStrategyKind::ClaimedHttps,
            vec![
                "https://acme.reports.example.com/callback",
                "https://acme.example.com/reports/callback"
            ]
        )
    );
    assert_eq!(
        projected[0],
        document.client().identity.clients[0],
        "the stored entry is its projection"
    );
    assert_eq!(document.projection_check(), ProjectionCheck::Consistent);
}

#[test]
fn a_client_with_no_hosts_still_takes_only_the_public_template() {
    let document = edited(|text| {
        text.replace(PUBLIC_HOSTS, "  hosts: []\n")
            .replace("        - https://acme.example.com/reports/callback\n", "")
    });

    assert_eq!(
        only(&document.projected_clients().unwrap()),
        (
            &RedirectStrategyKind::ClaimedHttps,
            vec!["https://acme.reports.example.com/callback"]
        )
    );
    assert_eq!(document.projection_check(), ProjectionCheck::Consistent);
}

#[test]
fn a_client_on_internal_hosts_is_projected_as_private_network_on_those_hosts_only() {
    let document = internal(INTERNAL_REDIRECT);

    assert_eq!(
        only(&document.projected_clients().unwrap()),
        (
            &RedirectStrategyKind::PrivateNetwork,
            vec![
                "http://acme.lucentroot.internal/reports/callback",
                "http://reports.acme.internal/reports/callback"
            ]
        ),
        "the public hostname template is never projected onto a private-network client"
    );
    assert_eq!(document.projection_check(), ProjectionCheck::Consistent);
}

#[test]
fn an_internal_client_still_holding_a_public_projection_diverges_on_its_strategy_and_is_withheld() {
    let document = edited(|text| text.replace(PUBLIC_HOSTS, INTERNAL_HOSTS));
    let check = document.projection_check();

    assert_eq!(
        check,
        ProjectionCheck::Diverged(vec![ProjectedClientDivergence {
            client: reports(),
            field: ProjectedClientField::RedirectStrategy,
        }])
    );
    assert_eq!(check.withheld(), [reports()]);
}

#[test]
fn an_internal_projection_with_https_an_extra_or_a_removed_callback_diverges_on_its_callbacks() {
    let edits = [
        INTERNAL_REDIRECT.replace("http://acme.lucentroot", "https://acme.lucentroot"),
        INTERNAL_REDIRECT.replace(
            "        - http://reports.acme.internal/reports/callback\n",
            "        - http://reports.acme.internal/reports/callback\n        - http://evil.internal/callback\n",
        ),
        INTERNAL_REDIRECT.replace("        - http://reports.acme.internal/reports/callback\n", ""),
    ];

    for redirect in edits {
        let check = internal(&redirect).projection_check();

        assert_eq!(
            check,
            ProjectionCheck::Diverged(vec![ProjectedClientDivergence {
                client: reports(),
                field: ProjectedClientField::RedirectUris,
            }]),
            "{redirect}"
        );
        assert_eq!(check.withheld(), [reports()]);
    }
}

#[test]
fn a_client_mixing_internal_and_public_hosts_is_refused_and_its_projection_is_unverifiable() {
    let document = edited(|text| {
        text.replace(
            PUBLIC_HOSTS,
            "  hosts:\n  - acme.example.com\n  - acme.lucentroot.internal\n",
        )
    });

    let Err(DesiredStateError::InvalidField { field, detail }) = document.projected_clients() else {
        panic!("a mixed client must not be projected");
    };
    assert_eq!(field, "hosts");
    assert!(
        detail.contains(".internal") && detail.contains("public"),
        "{detail}"
    );
    let check = document.projection_check();
    assert_eq!(
        check,
        ProjectionCheck::Unverifiable {
            unverified: vec![reports()]
        }
    );
    assert_eq!(check.withheld(), [reports()]);
}

#[test]
fn a_loopback_host_is_still_refused() {
    let document = edited(|text| text.replace(PUBLIC_HOSTS, "  hosts:\n  - localhost\n"));

    assert!(document.projected_clients().is_err());
}
