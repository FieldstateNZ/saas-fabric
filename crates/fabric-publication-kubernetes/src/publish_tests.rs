//! The adapter writes what the plan says, where and in the order it says,
//! and refuses before writing when it should.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::testing::{fake, object, snapshot};
use fabric_runtime_publication::{DocumentInput, DocumentManifest, FilesystemRuntimePublication};

const NS: &str = "platform-system";

fn manifest(document: DocumentKind, revision: u64) -> String {
    String::from_utf8(
        DocumentManifest::new(
            document,
            fabric_runtime_publication::DocumentRevision::new(revision),
        )
        .canonical_json()
        .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn a_first_publication_creates_three_objects_in_order_with_the_label() {
    let (client, _token, task) = fake(vec![
        (404, "{}".into()),
        (404, "{}".into()),
        (404, "{}".into()),
        (201, "{}".into()),
        (201, "{}".into()),
        (201, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    let report = adapter.publish(&snapshot(1)).await.unwrap();

    assert_eq!(report.data_sources, DocumentOutcome::Written);
    let seen = task.join().unwrap();
    let lines: Vec<&str> = seen.iter().map(|r| r.line.as_str()).collect();
    assert_eq!(
        lines,
        vec![
            "GET /api/v1/namespaces/platform-system/configmaps/fabric-runtime-tenants HTTP/1.1",
            "GET /api/v1/namespaces/platform-system/configmaps/fabric-runtime-data-sources HTTP/1.1",
            "GET /api/v1/namespaces/platform-system/configmaps/fabric-runtime-catalog HTTP/1.1",
            "POST /api/v1/namespaces/platform-system/configmaps HTTP/1.1",
            "POST /api/v1/namespaces/platform-system/configmaps HTTP/1.1",
            "POST /api/v1/namespaces/platform-system/configmaps HTTP/1.1",
        ]
    );
    let first: serde_json::Value = serde_json::from_str(&seen[3].body).unwrap();
    assert_eq!(first["metadata"]["name"], "fabric-runtime-data-sources");
    assert_eq!(
        first["metadata"]["labels"]["app.kubernetes.io/managed-by"],
        "saas-fabric"
    );
    assert!(first["metadata"].get("resourceVersion").is_none());
    assert!(first["data"]["data-sources.json"]
        .as_str()
        .unwrap()
        .contains("shared-1"));
    assert_eq!(
        first["data"]["data-sources.manifest.json"],
        manifest(DocumentKind::DataSources, 1)
    );
    let last: serde_json::Value = serde_json::from_str(&seen[5].body).unwrap();
    assert_eq!(last["metadata"]["name"], "fabric-runtime-tenants");
}

#[tokio::test]
async fn a_held_object_is_replaced_at_the_version_it_was_read_and_an_unchanged_one_is_left() {
    let first = snapshot(1);
    // Compute what the adapter would have written, to hold it back.
    let plan = fabric_runtime_publication::plan_publication(
        &first,
        &fabric_runtime_publication::HeldDocuments::default(),
    )
    .unwrap();
    let tenants = String::from_utf8(plan.tenants.bytes.clone()).unwrap();
    let sources = String::from_utf8(plan.data_sources.bytes.clone()).unwrap();
    let catalog = String::from_utf8(plan.catalog.bytes.clone()).unwrap();
    let (client, _token, task) = fake(vec![
        (
            200,
            object(
                "fabric-runtime-tenants",
                "11",
                &[
                    ("tenants.json", &tenants),
                    ("tenants.manifest.json", &manifest(DocumentKind::Tenants, 1)),
                ],
            ),
        ),
        (
            200,
            object(
                "fabric-runtime-data-sources",
                "12",
                &[
                    ("data-sources.json", &sources),
                    (
                        "data-sources.manifest.json",
                        &manifest(DocumentKind::DataSources, 1),
                    ),
                ],
            ),
        ),
        (
            200,
            object(
                "fabric-runtime-catalog",
                "13",
                &[
                    ("catalog.json", &catalog),
                    ("catalog.manifest.json", &manifest(DocumentKind::Catalog, 1)),
                ],
            ),
        ),
        (200, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);
    let mut changed = snapshot(2);
    changed.data_sources = DocumentInput::new(
        fabric_runtime_publication::DocumentRevision::new(1),
        first.data_sources.payload.clone(),
    );
    changed.catalog = DocumentInput::new(
        fabric_runtime_publication::DocumentRevision::new(1),
        first.catalog.payload.clone(),
    );

    let report = adapter.publish(&changed).await.unwrap();

    assert_eq!(report.data_sources, DocumentOutcome::Unchanged);
    assert_eq!(report.catalog, DocumentOutcome::Unchanged);
    assert_eq!(report.tenants, DocumentOutcome::Written);
    let seen = task.join().unwrap();
    assert_eq!(seen.len(), 4);
    assert_eq!(
        seen[3].line,
        "PUT /api/v1/namespaces/platform-system/configmaps/fabric-runtime-tenants HTTP/1.1"
    );
    let body: serde_json::Value = serde_json::from_str(&seen[3].body).unwrap();
    assert_eq!(body["metadata"]["resourceVersion"], "11");
    assert_eq!(
        body["data"]["tenants.manifest.json"],
        manifest(DocumentKind::Tenants, 2)
    );
}

#[tokio::test]
async fn a_conflict_from_the_cluster_is_unwritable_and_names_the_document() {
    let (client, _token, task) = fake(vec![
        (404, "{}".into()),
        (404, "{}".into()),
        (404, "{}".into()),
        (409, "private detail".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    let error = adapter.publish(&snapshot(1)).await.unwrap_err();

    assert!(
        matches!(
            error,
            PublicationError::Unwritable {
                document: DocumentKind::DataSources,
                ..
            }
        ),
        "{error}"
    );
    assert!(!error.to_string().contains("private detail"));
    task.join().unwrap();
}

#[tokio::test]
async fn a_stale_snapshot_is_refused_before_any_write() {
    let tenants = String::from_utf8(
        fabric_runtime_publication::tenants_canonical_json(&snapshot(1).tenants.payload).unwrap(),
    )
    .unwrap();
    let (client, _token, task) = fake(vec![
        (
            200,
            object(
                "fabric-runtime-tenants",
                "1",
                &[
                    ("tenants.json", &tenants),
                    ("tenants.manifest.json", &manifest(DocumentKind::Tenants, 5)),
                ],
            ),
        ),
        (404, "{}".into()),
        (404, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    let error = adapter.publish(&snapshot(2)).await.unwrap_err();

    assert!(matches!(error, PublicationError::StaleRevision { .. }), "{error}");
    assert_eq!(task.join().unwrap().len(), 3, "three reads, and not one write");
}

#[tokio::test]
async fn a_document_past_the_object_cap_is_refused_before_any_write() {
    let (client, _token, task) = fake(vec![(404, "{}".into()), (404, "{}".into()), (404, "{}".into())]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);
    let mut big = snapshot(1);
    let mut source = big.data_sources.payload[0].clone();
    for n in 0..6000 {
        source.labels.insert(format!("label-{n}"), "x".repeat(160));
    }
    big.data_sources.payload = vec![source];

    let error = adapter.publish(&big).await.unwrap_err();

    assert!(
        matches!(
            error,
            PublicationError::Unwritable {
                document: DocumentKind::DataSources,
                ..
            }
        ),
        "{error}"
    );
    assert_eq!(task.join().unwrap().len(), 3);
}

#[tokio::test]
async fn current_reports_held_revisions_and_a_manifest_without_a_payload() {
    let (client, _token, task) = fake(vec![
        (
            200,
            object(
                "fabric-runtime-tenants",
                "1",
                &[("tenants.manifest.json", &manifest(DocumentKind::Tenants, 7))],
            ),
        ),
        (404, "{}".into()),
        (404, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    let revisions = adapter.current().await.unwrap();
    assert_eq!(
        revisions.tenants,
        Some(fabric_runtime_publication::DocumentRevision::new(7))
    );
    assert_eq!(revisions.catalog, None);
    task.join().unwrap();

    // Publishing over it is refused: the manifest says something is held,
    // and the bytes are gone.
    let (client, _token, task) = fake(vec![
        (
            200,
            object(
                "fabric-runtime-tenants",
                "1",
                &[("tenants.manifest.json", &manifest(DocumentKind::Tenants, 7))],
            ),
        ),
        (404, "{}".into()),
        (404, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);
    let error = adapter.publish(&snapshot(8)).await.unwrap_err();
    assert!(
        matches!(
            error,
            PublicationError::HeldPayloadLost {
                document: DocumentKind::Tenants
            }
        ),
        "{error}"
    );
    task.join().unwrap();
}

#[tokio::test]
async fn the_bytes_written_equal_what_the_filesystem_adapter_writes() {
    let dir = std::env::temp_dir().join(format!("fabric-publication-equality-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let files = FilesystemRuntimePublication::new(
        dir.join("tenants.json"),
        dir.join("data-sources.json"),
        dir.join("catalog.json"),
    );
    files.publish(&snapshot(3)).await.unwrap();
    let (client, _token, task) = fake(vec![
        (404, "{}".into()),
        (404, "{}".into()),
        (404, "{}".into()),
        (201, "{}".into()),
        (201, "{}".into()),
        (201, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    adapter.publish(&snapshot(3)).await.unwrap();

    let seen = task.join().unwrap();
    for (index, (payload, manifest_file)) in [
        ("data-sources.json", "data-sources.manifest.json"),
        ("catalog.json", "catalog.manifest.json"),
        ("tenants.json", "tenants.manifest.json"),
    ]
    .into_iter()
    .enumerate()
    {
        let body: serde_json::Value = serde_json::from_str(&seen[3 + index].body).unwrap();
        assert_eq!(
            body["data"][payload],
            std::fs::read_to_string(dir.join(payload)).unwrap(),
            "{payload}"
        );
        assert_eq!(
            body["data"][manifest_file],
            std::fs::read_to_string(dir.join(manifest_file)).unwrap(),
            "{manifest_file}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_token_is_read_for_every_request_so_rotation_is_honoured() {
    let (client, token, task) = fake(vec![(404, "{}".into()); 6]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    adapter.current().await.unwrap();
    std::fs::write(&token.0, "rotated-token\n").unwrap();
    adapter.current().await.unwrap();

    let seen = task.join().unwrap();
    assert_eq!(seen.len(), 6);
    for request in &seen[..3] {
        assert_eq!(request.bearer.as_deref(), Some("test-token"), "{}", request.line);
    }
    for request in &seen[3..] {
        assert_eq!(
            request.bearer.as_deref(),
            Some("rotated-token"),
            "{}",
            request.line
        );
    }
}

#[tokio::test]
async fn an_oversized_document_written_last_is_refused_before_the_first_is_written() {
    // Data sources are written first and are fine; tenants are written last
    // and would be too big. If the size check ran only per write, the data
    // sources object would already be on the cluster when tenants refused --
    // a half-updated cluster, which is what checking every object up front
    // exists to prevent.
    let (client, _token, task) = fake(vec![(404, "{}".into()), (404, "{}".into()), (404, "{}".into())]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);
    let mut big = snapshot(1);
    let mut tenant = big.tenants.payload[0].clone();
    for n in 0..7000 {
        tenant
            .features
            .insert(format!("feature-{n}-{}", "x".repeat(140)), true);
    }
    big.tenants.payload = vec![tenant];

    let error = adapter.publish(&big).await.unwrap_err();

    assert!(
        matches!(
            error,
            PublicationError::Unwritable {
                document: DocumentKind::Tenants,
                ..
            }
        ),
        "{error}"
    );
    assert_eq!(task.join().unwrap().len(), 3, "three reads, and not one write");
}
