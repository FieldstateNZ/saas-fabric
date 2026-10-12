//! The stored-projection check (D01-19b), over the client fixture: a
//! consistent document, and the ways a Git edit makes it diverge or makes
//! its projection impossible to compute.

use crate::{ClientDocument, OidcClientId, ProjectedClientDivergence, ProjectedClientField, ProjectionCheck};

const FIXTURE: &str = include_str!("../../../tests/fixtures/client.yaml");
const TEMPLATE_CALLBACK: &str = "        - https://acme.reports.example.com/callback\n";
const HOST_CALLBACK: &str = "        - https://acme.example.com/reports/callback\n";

fn edited(edit: impl FnOnce(String) -> String) -> ClientDocument {
    ClientDocument::parse(&edit(FIXTURE.to_owned())).expect("the edited text must still parse")
}

fn reports() -> OidcClientId {
    OidcClientId::try_new("reports").unwrap()
}

fn diverged(field: ProjectedClientField) -> ProjectionCheck {
    ProjectionCheck::Diverged(vec![ProjectedClientDivergence {
        client: reports(),
        field,
    }])
}

#[test]
fn a_stored_projection_that_matches_is_consistent_and_withholds_nothing() {
    let check = ClientDocument::parse(FIXTURE).unwrap().projection_check();

    assert_eq!(check, ProjectionCheck::Consistent);
    assert!(check.withheld().is_empty());
}

#[test]
fn reordered_callbacks_are_not_a_divergence() {
    let document = edited(|text| {
        text.replace(
            &format!("{TEMPLATE_CALLBACK}{HOST_CALLBACK}"),
            &format!("{HOST_CALLBACK}{TEMPLATE_CALLBACK}"),
        )
    });

    assert_eq!(document.projection_check(), ProjectionCheck::Consistent);
}

#[test]
fn a_changed_added_or_removed_callback_diverges_and_is_withheld() {
    let edits: [fn(String) -> String; 3] = [
        |text| {
            text.replace(
                "https://acme.example.com/reports/callback",
                "https://evil.example.com/callback",
            )
        },
        |text| {
            text.replace(
                HOST_CALLBACK,
                &format!("{HOST_CALLBACK}        - https://extra.example.com/cb\n"),
            )
        },
        |text| text.replace(HOST_CALLBACK, ""),
    ];

    for edit in edits {
        let check = edited(edit).projection_check();

        assert_eq!(check, diverged(ProjectedClientField::RedirectUris));
        assert_eq!(check.withheld(), [reports()]);
    }
}

const PRIVATE: &str =
    "        strategy: privateNetwork\n        uris:\n        - https://reports.acme.internal/callback\n";

fn private_network(text: &str) -> String {
    text.replace(
        &format!("        strategy: claimedHttps\n        uris:\n{TEMPLATE_CALLBACK}{HOST_CALLBACK}"),
        PRIVATE,
    )
}

#[test]
fn a_changed_redirect_strategy_diverges() {
    let document = edited(|text| private_network(&text));

    assert_eq!(
        document.projection_check(),
        diverged(ProjectedClientField::RedirectStrategy)
    );
}

#[test]
fn a_removed_projected_entry_diverges_but_withholds_nothing_it_does_not_declare() {
    let document = edited(|text| {
        let start = text.find("    clients:\n").unwrap();
        let end = text.find("  product:\n").unwrap();
        format!("{}{}", &text[..start], &text[end..])
    });
    let check = document.projection_check();

    assert_eq!(check, diverged(ProjectedClientField::Missing));
    assert!(check.withheld().is_empty());
}

#[test]
fn a_hand_declared_client_is_never_compared() {
    let document = edited(|text| {
        text.replace(
            "  product:\n",
            "    - id: manual\n      type: oidc\n      pkce: s256\n      redirect:\n        strategy: claimedHttps\n        uris:\n        - https://manual.example.com/callback\n  product:\n",
        )
    });

    assert_eq!(document.projection_check(), ProjectionCheck::Consistent);
}

#[test]
fn every_diverged_entry_is_reported_not_only_the_first() {
    let document = edited(|text| {
        private_network(&text).replace(
            "    - applicationId: reports\n",
            "    - applicationId: missing-app\n      release:\n        version: 1\n        note: ''\n        publishedAt: 1\n        definition:\n          name: Missing\n          description: ''\n          domain: ''\n          components: []\n          features: []\n          plans: []\n          fields: []\n          navigation: []\n      planId: standard\n      configuration: {}\n    - applicationId: reports\n",
        )
    });

    let ProjectionCheck::Diverged(divergences) = document.projection_check() else {
        panic!("expected a divergence");
    };
    let fields: Vec<_> = divergences.iter().map(|d| (d.client.as_str(), d.field)).collect();
    assert_eq!(
        fields,
        [
            ("reports", ProjectedClientField::RedirectStrategy),
            ("missing-app", ProjectedClientField::Missing)
        ]
    );
}

#[test]
fn a_projection_that_cannot_be_computed_is_unverifiable_and_withholds_the_assigned_clients() {
    let document = edited(|text| {
        text.replace("  hosts:\n  - acme.example.com\n", "  hosts: []\n")
            .replace(
                "          domain: '{client}.reports.example.com'\n",
                "          domain: ''\n",
            )
    });
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
fn an_unreadable_product_section_withholds_every_declared_client() {
    let document = edited(|text| text.replace("    region: au\n", "    region: [au]\n"));

    assert!(document.product().is_err());
    assert_eq!(document.projection_check().withheld(), [reports()]);
}
