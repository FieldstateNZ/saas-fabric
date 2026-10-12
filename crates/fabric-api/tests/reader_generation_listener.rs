//! The reader-generation listener: off unless configured, one route, four
//! keys, and the generation resolution actually reads (G7).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod examples_support;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::Router;
use examples_support::served::served_router;
use examples_support::stub_connector::StubConnector;
use examples_support::{config, data_sources, tenants};
use fabric_api::config::AppConfig;
use fabric_api::reader_generation::{router, ReaderGenerationListener};
use fabric_core::{RecordDigest, READER_GENERATION_PATH};
use fabric_tenant_runtime::{
    build_published_runtime, GenerationOrigin, PublishedRuntime, PublishedState, PublishedStateSource,
    RuntimeConfig, RuntimeResolver, SourceError,
};
use http::{Method, Request, StatusCode};
use tower::ServiceExt as _;

const PUBLISHED: u8 = 1;
const LAST_KNOWN_GOOD: u8 = 2;
const UNRECORDED: u8 = 3;

/// Answers the example documents as generation 7, from the published files
/// or the copy, or with no record at all; or nothing, before `answer` is set.
struct Example {
    answer: AtomicU8,
}

#[async_trait]
impl PublishedStateSource for Example {
    async fn load(&self) -> Result<PublishedState, SourceError> {
        let origin = match self.answer.load(Ordering::SeqCst) {
            PUBLISHED => GenerationOrigin::Published,
            LAST_KNOWN_GOOD => GenerationOrigin::LastKnownGood {
                held_because: "tenants is not the one generation 8 names at /etc/fabric".to_owned(),
            },
            UNRECORDED => {
                return Ok(PublishedState::Unrecorded {
                    data_sources: Ok(data_sources()),
                    tenants: Ok(tenants()),
                })
            }
            _ => {
                return Err(SourceError::Unreadable {
                    origin: "example".to_owned(),
                    cause: "not yet".into(),
                })
            }
        };
        Ok(PublishedState::Generation {
            generation: 7,
            record_sha256: RecordDigest::from_bytes([0xab; 32]),
            origin,
            data_sources: data_sources(),
            tenants: tenants(),
        })
    }

    fn describe(&self) -> String {
        "example".to_owned()
    }
}

async fn runtime(answer: u8) -> Arc<RuntimeResolver> {
    let source = Arc::new(Example {
        answer: AtomicU8::new(answer),
    });
    let config = RuntimeConfig {
        fail_fast_on_prime: false,
        ..RuntimeConfig::default()
    };
    let PublishedRuntime {
        resolver, handles, ..
    } = build_published_runtime(&config, source as Arc<dyn PublishedStateSource>)
        .await
        .unwrap();
    handles.shutdown().await.unwrap();
    resolver
}

async fn send(router: Router, method: Method, path: &str) -> (StatusCode, http::HeaderMap, Vec<u8>) {
    let response = router
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 4096).await.unwrap().to_vec();
    (parts.status, parts.headers, bytes)
}

#[tokio::test]
async fn a_served_generation_is_answered_as_exactly_four_keys() {
    let (status, headers, body) = send(
        router(runtime(PUBLISHED).await),
        Method::GET,
        READER_GENERATION_PATH,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(headers["cache-control"], "no-store");
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "contract": 1,
            "generation": 7,
            "record_sha256": "ab".repeat(32),
            "origin": "published",
        })
    );
}

#[tokio::test]
async fn the_answer_names_no_tenant_data_source_path_or_hold_reason() {
    for answer in [PUBLISHED, LAST_KNOWN_GOOD] {
        let (_, _, body) = send(router(runtime(answer).await), Method::GET, READER_GENERATION_PATH).await;
        let text = String::from_utf8(body).unwrap();
        for binding in tenants() {
            assert!(!text.contains(binding.tenant.as_str()), "{text}");
        }
        for source in data_sources() {
            assert!(!text.contains(source.id.as_str()), "{text}");
        }
        for leak in ["/etc/fabric", "held_because", "tenants is not"] {
            assert!(!text.contains(leak), "{text}");
        }
    }
    let (_, _, body) = send(
        router(runtime(LAST_KNOWN_GOOD).await),
        Method::GET,
        READER_GENERATION_PATH,
    )
    .await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["origin"], "last_known_good");
}

#[tokio::test]
async fn nothing_served_whole_is_a_404_with_no_body() {
    for answer in [0, UNRECORDED] {
        let (status, _, body) =
            send(router(runtime(answer).await), Method::GET, READER_GENERATION_PATH).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "answer {answer}");
        assert!(body.is_empty());
    }
}

#[tokio::test]
async fn the_listener_serves_its_one_route_and_nothing_else() {
    let served = router(runtime(PUBLISHED).await);
    let (status, _, _) = send(served.clone(), Method::POST, READER_GENERATION_PATH).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    for path in [
        "/",
        "/ready",
        "/health",
        "/live",
        "/v1/data/customers",
        "/reader-generation/7",
    ] {
        let (status, _, _) = send(served.clone(), Method::GET, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn the_main_listener_has_no_reader_generation_route() {
    let stub = StubConnector::new(config().connectors.first().unwrap().id.as_str());
    let main = served_router(Duration::from_secs(30), Arc::new(stub));
    let (status, _, _) = send(main, Method::GET, READER_GENERATION_PATH).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn omitted_configuration_binds_nothing() {
    let resolver = runtime(PUBLISHED).await;
    assert!(ReaderGenerationListener::configured(&AppConfig::default(), &resolver).is_none());
    assert!(ReaderGenerationListener::configured(&config(), &resolver).is_none());
    let configured = AppConfig {
        reader_generation_listen: Some("0.0.0.0:9091".to_owned()),
        ..config()
    };
    let listener = ReaderGenerationListener::configured(&configured, &resolver).unwrap();
    assert_eq!(listener.address, "0.0.0.0:9091");
}
