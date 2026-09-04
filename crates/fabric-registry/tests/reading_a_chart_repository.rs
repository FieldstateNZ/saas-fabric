//! Reading a chart repository over its published index.
//!
//! Against a real HTTP server rather than a fake port, because everything
//! worth testing here is in the adapter: how much of a document it is willing
//! to read, what it does with entries it cannot order, and which URLs and
//! redirects it will and will not follow.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use fabric_platform_management::{ChartIndex, RegistryError};
use fabric_registry::HelmCharts;
use support::http_server::{self, RecordedRequest, Reply};

/// What a served fixture hands back: its base URL, and every request it has
/// recorded so far.
type Served = (String, Arc<Mutex<Vec<RecordedRequest>>>);

/// Starts a fake chart repository behind a custom responder.
async fn serving_with(responder: http_server::Responder) -> Served {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let base = http_server::start(responder, Arc::clone(&recorded)).await;
    (base, recorded)
}

/// Serves one index document at `/index.yaml`, and nothing else.
async fn serving(body: &str) -> Served {
    let body = body.to_owned();

    serving_with(Arc::new(move |request: &RecordedRequest| {
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
    }))
    .await
}

/// Serves `/index.yaml` as a redirect to `target`, and `/moved/index.yaml`
/// (the target a same-server redirect in these tests points at) with
/// [`KEYCLOAKX`].
async fn serving_redirecting_to(target: &str) -> Served {
    let target = target.to_owned();
    let body = KEYCLOAKX.to_owned();

    serving_with(Arc::new(move |request: &RecordedRequest| {
        match request.path.as_str() {
            "/index.yaml" => Reply::json(302, String::new()).with("Location", target.clone()),
            "/moved/index.yaml" => Reply::json(200, body.clone()),
            _ => Reply::json(404, String::new()),
        }
    }))
    .await
}

/// A reader built for the served, loopback fixtures above: the test server
/// has no certificate to offer, so this is the constructor that tolerates
/// plain HTTP, and only to loopback.
fn charts() -> HelmCharts {
    HelmCharts::plain_http_to_loopback(10).expect("the client builds")
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

const KEYCLOAK_AND_KEYCLOAKX: &str = r"apiVersion: v1
entries:
  keycloak:
    - version: 24.0.0
  keycloakx:
    - version: 7.3.1
";

#[tokio::test]
async fn every_version_of_the_named_chart_is_returned() {
    let (base, _recorded) = serving(KEYCLOAKX).await;

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
async fn keycloak_and_keycloakx_are_not_confused() {
    let (base, _recorded) = serving(KEYCLOAK_AND_KEYCLOAKX).await;

    let found = charts()
        .versions(&base, "keycloak")
        .await
        .expect("the index reads");

    let text: Vec<&str> = found
        .iter()
        .map(fabric_platform_management::Version::as_str)
        .collect();
    assert_eq!(text, vec!["24.0.0"], "not keycloakx's version");
}

#[tokio::test]
async fn a_chart_the_repository_does_not_publish_is_empty_rather_than_an_error() {
    let (base, _recorded) = serving(KEYCLOAKX).await;

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
    let (base, _recorded) = serving(
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
    let (base, _recorded) = serving(
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
async fn an_unrelated_charts_entry_that_is_a_scalar_does_not_break_discovery() {
    let (base, _recorded) = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0
  postgresql: not-a-list-at-all
",
    )
    .await;

    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("a scalar entry under another chart does not break this one");

    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn an_unrelated_chart_lacking_a_version_field_does_not_break_discovery() {
    let (base, _recorded) = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0
  postgresql:
    - appVersion: 16.0
",
    )
    .await;

    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("a missing version field on another chart does not break this one");

    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn an_unrelated_charts_version_that_is_a_list_does_not_break_discovery() {
    let (base, _recorded) = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0
  postgresql:
    - version: [1, 2, 3]
",
    )
    .await;

    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("a list-typed version on another chart does not break this one");

    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn an_unrelated_charts_version_that_is_a_number_does_not_break_discovery() {
    let (base, _recorded) = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0
  postgresql:
    - version: 5
",
    )
    .await;

    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("a numeric version on another chart does not break this one");

    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn an_unrelated_charts_version_that_is_a_map_does_not_break_discovery() {
    let (base, _recorded) = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0
  postgresql:
    - version: { major: 1, minor: 0 }
",
    )
    .await;

    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("a map-typed version on another chart does not break this one");

    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn an_entries_value_that_is_a_map_rather_than_a_list_does_not_break_discovery() {
    let (base, _recorded) = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0
  postgresql:
    latest: 16.0
",
    )
    .await;

    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("another chart's entries being a mapping, not a list, does not break this one");

    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn a_document_that_is_not_a_mapping_is_refused() {
    let (base, _recorded) = serving("- just\n- a\n- list\n").await;

    let failure = charts()
        .versions(&base, "keycloakx")
        .await
        .expect_err("a document that is not a mapping cannot carry an index");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn an_entries_key_that_is_not_a_mapping_is_refused() {
    let (base, _recorded) = serving(
        "apiVersion: v1
entries:
  - keycloakx
  - postgresql
",
    )
    .await;

    let failure = charts()
        .versions(&base, "keycloakx")
        .await
        .expect_err("entries that is not itself a mapping cannot be read");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn two_releases_of_equal_precedence_are_refused_rather_than_chosen_between() {
    // SemVer says build metadata is not part of precedence, so these two are
    // neither newer than the other. There is no newest, and picking would be
    // picking arbitrarily.
    let (base, _recorded) = serving(
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
    let (base, _recorded) = serving(
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

    let (base, _recorded) = serving(&huge).await;

    let failure = charts()
        .versions(&base, "keycloakx")
        .await
        .expect_err("a document past the bound is not read");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn an_unrelated_charts_alias_amplification_costs_neither_memory_nor_time() {
    // A neighbouring chart repeats one 100-key anchor 200,000 times, well
    // inside the byte bound. A reader that materialises every chart's
    // entries as a `Value` before picking the requested one re-inflates the
    // anchor on every alias, and does not just get slow: measured against
    // this exact fixture it allocated 6.28 GB and took 43 seconds. The
    // `DeserializeSeed` this reader now uses never turns an alias it is not
    // keeping into a value at all, so this should read in well under a
    // second -- if this test ever turns slow or memory-hungry, the fix has
    // regressed back toward materialising unrelated charts.
    let mut amplified = String::from(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.1
    - version: 7.3.0
  amplified-elsewhere:
",
    );

    {
        use std::fmt::Write as _;
        write!(amplified, "    - &a {{").expect("writing to a String cannot fail");
        for n in 0..100 {
            write!(amplified, "k{n}: {n}, ").expect("writing to a String cannot fail");
        }
        writeln!(amplified, "}}").expect("writing to a String cannot fail");

        for _ in 0..200_000 {
            writeln!(amplified, "    - *a").expect("writing to a String cannot fail");
        }
    }
    assert!(
        amplified.len() > 1024 * 1024,
        "the fixture has to be large enough for amplification to matter"
    );

    let (base, _recorded) = serving(&amplified).await;

    let started = std::time::Instant::now();
    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("only keycloakx's own entries are read");
    let elapsed = started.elapsed();

    let text: Vec<&str> = found
        .iter()
        .map(fabric_platform_management::Version::as_str)
        .collect();
    assert_eq!(
        text,
        vec!["7.3.1", "7.3.0"],
        "the amplified neighbour is never in the way"
    );

    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "reading past an alias-amplified neighbour took {elapsed:?}; the fix has regressed"
    );
}

#[tokio::test]
async fn the_requested_chart_listed_twice_under_entries_is_refused() {
    // Today's `BTreeMap`-based reader would insert the second occurrence
    // over the first and pick silently between them. A `DeserializeSeed`
    // sees both key/value pairs as they arrive off the wire, so it can, and
    // does, refuse instead.
    let (base, _recorded) = serving(
        "apiVersion: v1
entries:
  keycloakx:
    - version: 7.3.0
  keycloakx:
    - version: 8.0.0
",
    )
    .await;

    let failure = charts()
        .versions(&base, "keycloakx")
        .await
        .expect_err("a chart named twice under entries is not something to pick between");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn a_repository_that_refuses_is_a_registry_failure_and_not_an_empty_list() {
    let (base, _recorded) = serving("").await;

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

#[tokio::test]
async fn a_repository_that_never_answers_times_out_rather_than_hanging() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            // Held open and never answered, so nothing but the client's own
            // timeout below ends the read.
            let _stream = stream;
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    let short_timeout = HelmCharts::plain_http_to_loopback(1).expect("the client builds");

    let failure = short_timeout
        .versions(&format!("http://{address}"), "keycloakx")
        .await
        .expect_err("a server that never answers should time out rather than hang");

    assert!(
        matches!(failure, RegistryError::Unavailable { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn versions_reads_the_index_exactly_once() {
    let (base, recorded) = serving(KEYCLOAKX).await;

    charts()
        .versions(&base, "keycloakx")
        .await
        .expect("the index reads");

    let requests = recorded.lock().unwrap();
    assert_eq!(requests.len(), 1, "one request for one lookup");
    assert_eq!(requests[0].path, "/index.yaml");
}

#[tokio::test]
async fn plain_http_is_refused_before_any_connection_is_attempted() {
    // Port 1 is closed. If this reader had tried to connect, the failure
    // would be `Unavailable`; `Refused` proves the URL was rejected first.
    let failure = HelmCharts::new(10)
        .expect("the client builds")
        .versions("http://127.0.0.1:1", "keycloakx")
        .await
        .expect_err("plain HTTP is not read, even to loopback");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn plain_http_to_loopback_still_refuses_a_non_loopback_host() {
    // No DNS lookup or connection is attempted for a non-loopback plain HTTP
    // host, even under the loopback-tolerant test policy -- `Refused` proves
    // that; `Unavailable` would mean a connection was tried and failed.
    let failure = HelmCharts::plain_http_to_loopback(10)
        .expect("the client builds")
        .versions("http://charts.example.test", "keycloakx")
        .await
        .expect_err("a non-loopback host is refused under the loopback policy too");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn a_redirect_on_the_same_loopback_server_is_followed() {
    let (base, recorded) = serving_redirecting_to("/moved/index.yaml").await;

    let found = charts()
        .versions(&base, "keycloakx")
        .await
        .expect("a same-host redirect on loopback is followed");

    assert_eq!(found.len(), 2);
    assert_eq!(
        recorded.lock().unwrap().len(),
        2,
        "the initial request and the followed redirect"
    );
}

#[tokio::test]
async fn a_redirect_off_loopback_is_refused() {
    let (base, recorded) = serving_redirecting_to("http://example.invalid/index.yaml").await;

    let failure = charts()
        .versions(&base, "keycloakx")
        .await
        .expect_err("a redirect off loopback is not followed");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
    assert_eq!(
        recorded.lock().unwrap().len(),
        1,
        "the redirect target was never asked"
    );
}

#[tokio::test]
async fn a_redirect_that_upgrades_to_https_is_permitted_and_then_fails_to_connect() {
    // Proves the policy actually lets an HTTPS hop through: if it did not,
    // this would fail as `Refused` rather than `Unavailable`. Port 1 is
    // closed, so the upgraded request can never itself succeed.
    let (base, _recorded) = serving_redirecting_to("https://127.0.0.1:1/index.yaml").await;

    let failure = charts()
        .versions(&base, "keycloakx")
        .await
        .expect_err("nothing listens on the upgraded port");

    assert!(
        matches!(failure, RegistryError::Unavailable { .. }),
        "{failure:?}"
    );
}
