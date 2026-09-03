//! Reading a chart repository over its published index.
//!
//! Against a real HTTP server rather than a fake port, because everything
//! worth testing here is in the adapter: how much of a document it is willing
//! to read, and what it does with entries it cannot order.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::{Arc, Mutex};

use fabric_platform_management::{ChartIndex, RegistryError};
use fabric_registry::HelmCharts;
use support::http_server::{self, Reply};

/// Serves one index document at `/index.yaml`.
async fn serving(body: &str) -> String {
    let body = body.to_owned();

    http_server::start(
        Arc::new(move |request: &http_server::RecordedRequest| {
            if request.path == "/index.yaml" {
                Reply {
                    status: 200,
                    headers: Vec::new(),
                    body: body.clone(),
                }
            } else {
                Reply {
                    status: 404,
                    headers: Vec::new(),
                    body: String::new(),
                }
            }
        }),
        Arc::new(Mutex::new(Vec::new())),
    )
    .await
}

fn charts() -> HelmCharts {
    HelmCharts::new(10).expect("the client builds")
}

const KEYCLOAKX: &str = r"apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.1
      appVersion: 26.7.3
    - version: 7.3.0
      appVersion: 26.7.2
  postgresql:
    - version: 1.0.0
";

#[tokio::test]
async fn every_version_of_the_named_chart_is_returned() {
    let base = serving(KEYCLOAKX).await;

    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("the index reads");

    let text: Vec<&str> = found
        .iter()
        .map(fabric_platform_management::Version::as_str)
        .collect();
    assert_eq!(text, vec!["7.3.1", "7.3.0"], "and only that chart's");
}

#[tokio::test]
async fn a_chart_the_repository_does_not_publish_is_empty_rather_than_an_error() {
    let base = serving(KEYCLOAKX).await;

    let found = charts()
        .versions(&base, "nothing-like-this")
        .await
        .expect("an index that does not carry a chart still reads");

    assert!(found.is_empty());
}

#[tokio::test]
async fn a_version_this_cannot_read_is_refused_rather_than_skipped() {
    // Skipping would mean answering "the newest is 7.3.0" while holding
    // something that might have been newer. A wrong answer given confidently
    // is worse than no answer.
    let base = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0
    - version: not-a-version
",
    )
    .await;

    let failure = charts()
        .versions(&base, "keycloakx")
        .await
        .expect_err("an unreadable version is not something to order around");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn another_charts_malformed_entry_is_not_this_components_problem() {
    // A repository serves every chart it holds. Refusing to read keycloakx
    // because somebody else's chart has a strange version would make a
    // component unmanageable for a reason that has nothing to do with it.
    let base = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0
  somebody-else:
    - version: not-a-version
",
    )
    .await;

    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("only this chart's entries are read");

    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn two_releases_of_equal_precedence_are_refused_rather_than_chosen_between() {
    // SemVer says build metadata is not part of precedence, so these two are
    // neither newer than the other. There is no newest, and picking would be
    // picking arbitrarily.
    let base = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0+a
    - version: 7.3.0+b
",
    )
    .await;

    let failure = charts()
        .versions(&base, "keycloakx")
        .await
        .expect_err("there is no newest of two equal versions");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn build_metadata_survives_into_what_would_be_pinned() {
    let base = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0+build.7
",
    )
    .await;

    let found = charts().versions(&base, "keycloakx").await.expect("reads");

    assert_eq!(found[0].as_str(), "7.3.0+build.7");
}

#[tokio::test]
async fn an_index_larger_than_the_bound_is_refused_rather_than_held() {
    // An unbounded read of a remote document is an unbounded allocation
    // decided by somebody else.
    let mut huge = String::from("apiVersion: v1\nentries:\n  keycloakx:\n");
    // Each entry is tiny; enough of them exceeds eight megabytes.
    for n in 0..200_000 {
        use std::fmt::Write as _;
        let _ = writeln!(
            huge,
            "    - version: 1.0.{n}\n      description: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
        );
    }
    assert!(
        huge.len() > 8 * 1024 * 1024,
        "the fixture has to exceed the bound"
    );

    let base = serving(&huge).await;

    let failure = charts()
        .versions(&base, "keycloakx")
        .await
        .expect_err("a document past the bound is not read");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn a_repository_that_refuses_is_a_registry_failure_and_not_an_empty_list() {
    let base = serving("").await;

    let failure = charts()
        .versions(&format!("{base}/missing"), "keycloakx")
        .await
        .expect_err("a 404 is not an answer");

    assert!(
        matches!(
            failure,
            RegistryError::Unavailable { .. } | RegistryError::Refused { .. }
        ),
        "{failure:?}"
    );
}
