//! D01-15: a projected client follows the kind of the client's hosts. A
//! client on `.internal` hosts takes an application as a `privateNetwork`
//! client on those hosts, over the scheme it declares, and is never given the
//! release's public template callback. Nothing infers the scheme.
//!
//! The D01-19b semantics hold unchanged around it: every read reports the
//! stored projection, a sweep withholds a diverged or unverifiable entry and
//! leaves identity readiness unmet, and a re-assertion onto one is refused.
//! How a converged client moves between hosts is `host_transition.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeSet;

use http::StatusCode;
use serde_json::json;
use support::control_plane;
use support::newco::{
    analytics_in_provider, converged_on, get, identity_unmet, internal_callback, provider_callbacks,
    put_product_on, reassert, reconciliation, refusal, reported_projection, save_product_on, settled,
    stored_redirect, sweep, INTERNAL_HOST, PUBLIC_CALLBACKS,
};
use support::projected::{hand_edit_in_git, newco_on_hosts, newco_with_a_projected_client};

#[tokio::test]
async fn an_internal_client_takes_an_application_over_the_scheme_it_declares() {
    for scheme in ["http", "https"] {
        let plane = control_plane();
        newco_on_hosts(&plane.router, &[INTERNAL_HOST], Some(scheme)).await;

        let redirect = stored_redirect(&plane.router).await;

        assert_eq!(
            redirect,
            json!({"strategy": "privateNetwork", "uris": [internal_callback(scheme)]})
        );
        assert!(
            !redirect.to_string().contains("example.com"),
            "the release's public template is not projected onto an internal client"
        );
        assert_eq!(
            get(&plane.router, "/api/clients/newco").await["privateNetworkScheme"],
            scheme
        );
        assert_eq!(
            reported_projection(&plane.router).await,
            json!({"state": "consistent"})
        );
    }
}

#[tokio::test]
async fn an_internal_client_with_no_declared_scheme_is_refused_not_given_http() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;

    let (status, error) = refusal(put_product_on(&plane.router, &[INTERNAL_HOST], None).await).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(error["code"], "invalid_request");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("privateNetworkScheme"),
        "{error}"
    );
    assert_eq!(
        stored_redirect(&plane.router).await,
        json!({"strategy": "claimedHttps", "uris": PUBLIC_CALLBACKS}),
        "nothing was written"
    );
}

#[tokio::test]
async fn a_scheme_on_a_public_client_is_refused() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;

    let (status, error) =
        refusal(put_product_on(&plane.router, &["newco.example.com"], Some("http")).await).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert!(error["message"]
        .as_str()
        .unwrap()
        .contains("only to a client whose hosts are all .internal"));
}

#[tokio::test]
async fn an_unknown_scheme_is_refused_before_anything_is_written() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;

    let (status, _) = refusal(put_product_on(&plane.router, &[INTERNAL_HOST], Some("ftp")).await).await;

    assert!(status.is_client_error(), "{status}");
    assert_eq!(
        stored_redirect(&plane.router).await,
        json!({"strategy": "claimedHttps", "uris": PUBLIC_CALLBACKS})
    );
}

#[tokio::test]
async fn a_public_client_is_projected_exactly_as_before() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;

    assert_eq!(
        stored_redirect(&plane.router).await,
        json!({"strategy": "claimedHttps", "uris": PUBLIC_CALLBACKS})
    );
    assert_eq!(
        get(&plane.router, "/api/clients/newco").await["privateNetworkScheme"],
        json!(null)
    );
    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
}

#[tokio::test]
async fn an_internal_client_converges_with_only_its_private_callbacks_and_meets_identity() {
    let (plane, provider) = converged_on(&[INTERNAL_HOST], Some("https")).await;

    let analytics = analytics_in_provider(&provider);
    assert_eq!(
        provider_callbacks(&provider),
        BTreeSet::from([internal_callback("https")])
    );
    assert!(analytics.public);
    let reconciliation = reconciliation(&plane.router).await;
    assert_eq!(reconciliation["status"], "applied", "{reconciliation}");
    assert_eq!(reconciliation["withheld"], json!([]));
    assert!(!identity_unmet(&plane.router).await);
}

#[tokio::test]
async fn a_product_save_moving_hosts_between_kinds_reprojects_and_stays_consistent() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;

    save_product_on(&plane.router, &[INTERNAL_HOST], Some("http")).await;
    assert_eq!(
        stored_redirect(&plane.router).await,
        json!({"strategy": "privateNetwork", "uris": [internal_callback("http")]})
    );
    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );

    save_product_on(&plane.router, &["newco.example.com"], None).await;
    assert_eq!(
        stored_redirect(&plane.router).await,
        json!({"strategy": "claimedHttps", "uris": PUBLIC_CALLBACKS})
    );
    assert_eq!(
        get(&plane.router, "/api/clients/newco").await["privateNetworkScheme"],
        json!(null),
        "the scheme goes with the .internal hosts"
    );
    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
}

#[tokio::test]
async fn a_git_edit_to_an_internal_projection_is_reported_withheld_and_refuses_reassertion() {
    let (plane, provider) = converged_on(&[INTERNAL_HOST], Some("http")).await;
    let before = analytics_in_provider(&provider);
    hand_edit_in_git(&plane, |text| {
        text.replace(&internal_callback("http"), &internal_callback("https"))
    })
    .await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "diverged", "divergences": [{"client": "analytics", "field": "redirectUris"}]})
    );
    sweep(&plane.router).await;
    assert_eq!(
        analytics_in_provider(&provider),
        before,
        "the Git edit is not converged"
    );
    let reconciliation = reconciliation(&plane.router).await;
    assert_eq!(reconciliation["status"], "withheld", "{reconciliation}");
    assert_eq!(reconciliation["withheld"], json!(["analytics"]));
    assert!(identity_unmet(&plane.router).await);

    let (status, error) = refusal(reassert(&plane.router).await).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "projected_client_diverged");
}

#[tokio::test]
async fn a_git_edit_moving_a_public_client_onto_internal_hosts_alone_diverges_and_is_withheld() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    let before = analytics_in_provider(&provider);
    hand_edit_in_git(&plane, |text| {
        text.replace(
            "  - newco.example.com\n",
            &format!("  - {INTERNAL_HOST}\n  privateNetworkScheme: http\n"),
        )
    })
    .await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "diverged", "divergences": [{"client": "analytics", "field": "redirectStrategy"}]})
    );
    sweep(&plane.router).await;
    assert_eq!(analytics_in_provider(&provider), before);
    assert_eq!(reconciliation(&plane.router).await["status"], "withheld");
    assert!(identity_unmet(&plane.router).await);
    let (status, error) = refusal(put_product_on(&plane.router, &[INTERNAL_HOST], Some("http")).await).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        error["code"], "projected_client_diverged",
        "a product save does not silently replace the diverged entry"
    );
}

#[tokio::test]
async fn a_git_edit_mixing_host_kinds_is_unverifiable_and_withheld() {
    let (plane, provider) = converged_on(&["newco.example.com"], None).await;
    let before = analytics_in_provider(&provider);
    hand_edit_in_git(&plane, |text| {
        text.replace(
            "  - newco.example.com\n",
            &format!("  - newco.example.com\n  - {INTERNAL_HOST}\n"),
        )
    })
    .await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "unverifiable", "unverified": ["analytics"]})
    );
    sweep(&plane.router).await;
    assert_eq!(analytics_in_provider(&provider), before);
    assert_eq!(reconciliation(&plane.router).await["status"], "withheld");
    assert!(identity_unmet(&plane.router).await);
    let (status, error) = refusal(put_product_on(&plane.router, &["newco.example.com"], None).await).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        error["code"], "desired_state_invalid",
        "an unverifiable stored projection is repaired in Git, as before"
    );
}

#[tokio::test]
async fn a_document_written_before_the_scheme_is_held_and_migrated_by_a_save_that_declares_one() {
    let (plane, provider) = converged_on(&[INTERNAL_HOST], Some("http")).await;
    let before = analytics_in_provider(&provider);
    hand_edit_in_git(&plane, |text| text.replace("  privateNetworkScheme: http\n", "")).await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "unverifiable", "unverified": ["analytics"]}),
        "never read as http because its callbacks happen to be"
    );
    sweep(&plane.router).await;
    assert_eq!(analytics_in_provider(&provider), before);
    assert_eq!(reconciliation(&plane.router).await["status"], "withheld");
    let (status, error) = refusal(put_product_on(&plane.router, &[INTERNAL_HOST], None).await).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    assert_eq!(
        reassert(&plane.router).await.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let revision = save_product_on(&plane.router, &[INTERNAL_HOST], Some("https")).await;
    settled(&plane, &revision).await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
    assert_eq!(
        provider_callbacks(&provider),
        BTreeSet::from([internal_callback("https")]),
        "the operator's explicit choice is what is written"
    );
    assert_eq!(reconciliation(&plane.router).await["status"], "applied");
}

#[tokio::test]
async fn a_legacy_document_whose_entries_were_edited_in_git_is_not_migrated_over_them() {
    let (plane, _provider) = converged_on(&[INTERNAL_HOST], Some("http")).await;
    hand_edit_in_git(&plane, |text| {
        text.replace("  privateNetworkScheme: http\n", "")
            .replace(&internal_callback("http"), "http://evil.internal/callback")
    })
    .await;

    let (status, error) = refusal(put_product_on(&plane.router, &[INTERNAL_HOST], Some("http")).await).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{error}");
    assert_eq!(error["code"], "desired_state_invalid");
}
