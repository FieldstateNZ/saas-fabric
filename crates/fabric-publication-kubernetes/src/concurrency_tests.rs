//! Overlapping and interrupted publications against a stateful fake API
//! server that enforces `resourceVersion` the way the real one does.
//!
//! `testing::fake` replays a script, so it cannot show what two writers do to
//! one another. The fake here keeps the three objects, answers `GET`, `POST`
//! and `PUT` against them, refuses a `PUT` whose `resourceVersion` is not the
//! stored one with 409, and can hold one writer's first write until the test
//! releases it. That makes the interleaving deterministic. Tests whose name
//! starts with `current_behaviour_` pin a gap named in
//! `docs/roadmap/m2-publication-gap-report.md`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, TenantId};
use fabric_runtime_publication::{
    CatalogDocument, CollectionName, ConnectionName, ConnectionSelectorDocument, ConnectorId,
    DataResidencyDocument, DataSourceCapabilitiesDocument, DataSourceDocument, DocumentInput, DocumentKind,
    DocumentOutcome, DocumentRevision, FieldName, IsolationModelDocument, PlacementClassDocument,
    PoolSettingsDocument, PublicationError, PublishedRevisions, ResourceDefinitionDocument,
    RuntimePublication as _, RuntimeSnapshot, TenantBindingDocument, TenantDataBindingDocument,
    TenantDataBindings,
};

use crate::client::Client;
use crate::testing::Token;
use crate::KubernetesRuntimePublication;

const NS: &str = "platform-system";

struct Hold {
    bearer: String,
    /// `None` holds the writer's first write; `Some(name)` holds its first
    /// `GET` of that object instead.
    read_of: Option<String>,
    reached: tokio::sync::oneshot::Sender<()>,
    release: mpsc::Receiver<()>,
}

#[derive(Default)]
struct State {
    objects: BTreeMap<String, (u64, serde_json::Value)>,
    hold: Option<Hold>,
    fail_write_to: Option<String>,
}

struct Cluster {
    base: String,
    state: Arc<Mutex<State>>,
}

impl Cluster {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let versions = Arc::new(AtomicU64::new(100));
        let shared = Arc::clone(&state);
        std::thread::spawn(move || {
            for socket in listener.incoming() {
                let Ok(socket) = socket else { return };
                let state = Arc::clone(&shared);
                let versions = Arc::clone(&versions);
                std::thread::spawn(move || serve(socket, &state, &versions));
            }
        });
        Self { base, state }
    }

    fn writer(&self, bearer: &str) -> (KubernetesRuntimePublication, Token) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let token = Token(std::env::temp_dir().join(format!(
            "fabric-publication-concurrency-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::write(&token.0, bearer).unwrap();
        let client = Client {
            http: reqwest::Client::new(),
            base: self.base.clone(),
            token_file: token.0.clone(),
        };
        (KubernetesRuntimePublication::with_client(client, NS), token)
    }

    /// Holds the first write `bearer` sends until the returned sender is
    /// used; the receiver fires once that write has arrived.
    fn hold_first_write_of(&self, bearer: &str) -> (tokio::sync::oneshot::Receiver<()>, mpsc::Sender<()>) {
        let (reached, reached_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release) = mpsc::channel();
        self.state.lock().unwrap().hold = Some(Hold {
            bearer: bearer.to_owned(),
            read_of: None,
            reached,
            release,
        });
        (reached_rx, release_tx)
    }

    /// Holds `bearer`'s first `GET` of `name` -- so after it has read every
    /// object before `name` and before it plans -- until the returned sender
    /// is used.
    fn hold_first_read_of(
        &self,
        bearer: &str,
        name: &str,
    ) -> (tokio::sync::oneshot::Receiver<()>, mpsc::Sender<()>) {
        let (reached, reached_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release) = mpsc::channel();
        self.state.lock().unwrap().hold = Some(Hold {
            bearer: bearer.to_owned(),
            read_of: Some(name.to_owned()),
            reached,
            release,
        });
        (reached_rx, release_tx)
    }

    fn fail_next_write_to(&self, name: &str) {
        self.state.lock().unwrap().fail_write_to = Some(name.to_owned());
    }

    fn data(&self, name: &str, key: &str) -> String {
        let state = self.state.lock().unwrap();
        state.objects[name].1["data"][key].as_str().unwrap().to_owned()
    }
}

fn serve(mut socket: TcpStream, state: &Mutex<State>, versions: &AtomicU64) {
    let mut head = Vec::new();
    let mut byte = [0];
    while !head.ends_with(b"\r\n\r\n") {
        if socket.read_exact(&mut byte).is_err() {
            return;
        }
        head.extend_from_slice(&byte);
    }
    let head = String::from_utf8(head).unwrap();
    let header = |name: &str| {
        head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim().to_owned())
        })
    };
    let length = header("content-length").map_or(0, |v| v.parse::<usize>().unwrap());
    let mut body = vec![0; length];
    socket.read_exact(&mut body).unwrap();
    let bearer = header("authorization")
        .and_then(|v| v.strip_prefix("Bearer ").map(str::to_owned))
        .unwrap_or_default();
    let mut words = head.split_whitespace();
    let method = words.next().unwrap().to_owned();
    let path = words.next().unwrap().to_owned();

    let (status, reply) = answer(&method, &path, &bearer, &body, state, versions);
    let _ = write!(
        socket,
        "HTTP/1.1 {status} Fake\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
}

fn answer(
    method: &str,
    path: &str,
    bearer: &str,
    body: &[u8],
    state: &Mutex<State>,
    versions: &AtomicU64,
) -> (u16, String) {
    let target = path.rsplit('/').next().unwrap();
    let paused = {
        let mut state = state.lock().unwrap();
        let matches = state.hold.as_ref().is_some_and(|hold| {
            hold.bearer == bearer
                && match (&hold.read_of, method) {
                    (None, "GET") => false,
                    (None, _) => true,
                    (Some(name), "GET") => name == target,
                    (Some(_), _) => false,
                }
        });
        if matches {
            state.hold.take()
        } else {
            None
        }
    };
    if let Some(paused) = paused {
        let _ = paused.reached.send(());
        paused.release.recv().unwrap();
    }

    let mut state = state.lock().unwrap();
    let name = path.rsplit('/').next().unwrap().to_owned();
    match method {
        "GET" => match state.objects.get(&name) {
            Some((version, object)) => {
                let mut object = object.clone();
                object["metadata"]["resourceVersion"] = serde_json::json!(version.to_string());
                (200, object.to_string())
            }
            None => (404, "{}".to_owned()),
        },
        "POST" | "PUT" => {
            let object: serde_json::Value = serde_json::from_slice(body).unwrap();
            let name = object["metadata"]["name"].as_str().unwrap().to_owned();
            if state.fail_write_to.as_deref() == Some(name.as_str()) {
                state.fail_write_to = None;
                return (500, "{}".to_owned());
            }
            let held = state.objects.get(&name).map(|(version, _)| version.to_string());
            let sent = object["metadata"]["resourceVersion"].as_str().map(str::to_owned);
            let accepted = match (method, held, sent) {
                ("POST", None, _) => 201,
                ("PUT", Some(held), Some(sent)) if held == sent => 200,
                ("PUT", None, _) => return (404, "{}".to_owned()),
                _ => return (409, "{}".to_owned()),
            };
            let version = versions.fetch_add(1, Ordering::SeqCst);
            state.objects.insert(name, (version, object));
            (accepted, "{}".to_owned())
        }
        _ => (405, "{}".to_owned()),
    }
}

fn data_source(id: &str) -> DataSourceDocument {
    DataSourceDocument {
        id: DataSourceId::try_new(id).unwrap(),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new(id).unwrap(),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        labels: BTreeMap::new(),
    }
}

fn tenant(id: &str, data_source: &str, revision: u64) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new(data_source).unwrap(),
            isolation: IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: id.to_owned(),
            },
        },
    );
    TenantBindingDocument {
        tenant: TenantId::try_new(id).unwrap(),
        revision: BindingRevision::new(revision),
        data: TenantDataBindings::try_new(data).unwrap(),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    }
}

fn snapshot(
    tenants: (u64, Vec<TenantBindingDocument>),
    data_sources: (u64, Vec<DataSourceDocument>),
) -> RuntimeSnapshot {
    let mut resources = BTreeMap::new();
    resources.insert(
        LogicalResourceName::try_new("customers").unwrap(),
        ResourceDefinitionDocument {
            data_source: LogicalDataSourceName::try_new("primary").unwrap(),
            collection: CollectionName::try_new("customers").unwrap(),
            key_field: FieldName::try_new("id").unwrap(),
            operations: vec![fabric_core::OperationKind::Read],
            queryable_fields: vec![],
        },
    );
    RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(tenants.0), tenants.1),
        data_sources: DocumentInput::new(DocumentRevision::new(data_sources.0), data_sources.1),
        catalog: DocumentInput::new(DocumentRevision::new(1), CatalogDocument::new(resources)),
    }
}

/// Held: acme on `pg-1`; data sources `pg-1` and `pg-2`, nothing on `pg-2`.
fn seed() -> RuntimeSnapshot {
    snapshot(
        (1, vec![tenant("acme", "pg-1", 1)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    )
}

/// Gap G4 (write skew across objects), the window before a plan is acted on:
/// fixed. Writer A reads the tenants and data-sources objects and is held
/// before it finishes reading; writer B moves acme onto `pg-2` and
/// publishes. A's plan to retire `pg-2` is valid against the tenants object
/// it read, but A re-reads the objects it relied on and will not write
/// (`confirm.rs`) before its first write, sees the tenants object moved, and
/// refuses without writing. Its re-offer (which the controller makes at once,
/// G3) re-reads and is refused by the retirement guard. The cluster never
/// holds a dangling binding.
#[tokio::test]
async fn a_writer_whose_read_only_object_moved_before_it_writes_is_refused() {
    let cluster = Cluster::start();
    let (seeder, _seed_token) = cluster.writer("seed");
    seeder.publish(&seed()).await.unwrap();

    let (a, _a_token) = cluster.writer("writer-a");
    let (b, _b_token) = cluster.writer("writer-b");
    let (reached, release) = cluster.hold_first_read_of("writer-a", "fabric-runtime-catalog");

    let retire = snapshot(
        (1, vec![tenant("acme", "pg-1", 1)]),
        (2, vec![data_source("pg-1")]),
    );
    let writer_a = tokio::spawn(async move {
        let first = a.publish(&retire).await;
        let reoffer = a.publish(&retire).await;
        (first, reoffer)
    });
    reached.await.unwrap();

    let rebind = snapshot(
        (2, vec![tenant("acme", "pg-2", 2)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    b.publish(&rebind).await.unwrap();

    release.send(()).unwrap();
    let (first, reoffer) = writer_a.await.unwrap();
    let error = first.unwrap_err();
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
    let error = reoffer.unwrap_err();
    assert!(
        matches!(error, PublicationError::RetiredDataSourceStillBound { .. }),
        "{error}"
    );

    assert!(cluster
        .data("fabric-runtime-tenants", "tenants.json")
        .contains("pg-2"));
    assert!(cluster
        .data("fabric-runtime-data-sources", "data-sources.json")
        .contains("pg-2"));
}

/// Gap G4 (write skew across objects), residual. The re-read above runs
/// before a writer's first write, so two writers that both pass it before
/// either writes still each write only their own object. Writer A retires
/// `pg-2` and is held at its write, after its check; writer B moves acme onto
/// `pg-2`, and its own check passes because A's write has not landed. Both
/// succeed, and the cluster holds a binding to a DataSource that is not
/// published. Closing this needs one writer (a `Lease`, whose RBAC is D01-1)
/// or a cross-document generation (G1b, D01-1).
#[tokio::test]
async fn current_behaviour_writers_overlapping_after_their_checks_can_leave_a_dangling_binding_in_the_cluster(
) {
    let cluster = Cluster::start();
    let (seeder, _seed_token) = cluster.writer("seed");
    seeder.publish(&seed()).await.unwrap();

    let (a, _a_token) = cluster.writer("writer-a");
    let (b, _b_token) = cluster.writer("writer-b");
    let (reached, release) = cluster.hold_first_write_of("writer-a");

    let retire = snapshot(
        (1, vec![tenant("acme", "pg-1", 1)]),
        (2, vec![data_source("pg-1")]),
    );
    let writer_a = tokio::spawn(async move { a.publish(&retire).await });
    reached.await.unwrap();

    let rebind = snapshot(
        (2, vec![tenant("acme", "pg-2", 2)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    let report_b = b.publish(&rebind).await.unwrap();
    assert_eq!(report_b.tenants, DocumentOutcome::Written);
    assert_eq!(report_b.data_sources, DocumentOutcome::Unchanged);

    release.send(()).unwrap();
    let report_a = writer_a.await.unwrap().unwrap();
    assert_eq!(report_a.data_sources, DocumentOutcome::Written);
    assert_eq!(report_a.tenants, DocumentOutcome::Unchanged);

    assert!(cluster
        .data("fabric-runtime-tenants", "tenants.json")
        .contains("pg-2"));
    assert!(!cluster
        .data("fabric-runtime-data-sources", "data-sources.json")
        .contains("pg-2"));
}

/// Within one object the API server's optimistic concurrency does what the
/// divergence guard cannot do for overlapping writers: the second writer of
/// the same object is refused with a 409 and reports `Unwritable`.
#[tokio::test]
async fn a_second_writer_of_the_same_object_is_refused_by_its_resource_version() {
    let cluster = Cluster::start();
    let (seeder, _seed_token) = cluster.writer("seed");
    seeder.publish(&seed()).await.unwrap();

    let (a, _a_token) = cluster.writer("writer-a");
    let (b, _b_token) = cluster.writer("writer-b");
    let (reached, release) = cluster.hold_first_write_of("writer-a");

    let first = snapshot(
        (2, vec![tenant("acme", "pg-1", 1), tenant("globex", "pg-1", 1)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    let writer_a = tokio::spawn(async move { a.publish(&first).await });
    reached.await.unwrap();

    let second = snapshot(
        (2, vec![tenant("acme", "pg-1", 1), tenant("initech", "pg-2", 1)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    b.publish(&second).await.unwrap();

    release.send(()).unwrap();
    let error = writer_a.await.unwrap().unwrap_err();
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
    assert!(cluster
        .data("fabric-runtime-tenants", "tenants.json")
        .contains("initech"));
}

/// Gap G3 (interrupted publication). A failure after the data-sources object
/// is replaced and before the tenants object is leaves the cluster at mixed
/// document revisions, which `current()` reports and every mounting pod's
/// kubelet will project. The next publication of the same snapshot finishes
/// the job.
#[tokio::test]
async fn current_behaviour_an_interrupted_publication_leaves_the_cluster_at_mixed_revisions() {
    let cluster = Cluster::start();
    let (writer, _token) = cluster.writer("writer");
    writer.publish(&seed()).await.unwrap();

    let next = snapshot(
        (2, vec![tenant("acme", "pg-2", 2)]),
        (
            2,
            vec![data_source("pg-1"), data_source("pg-2"), data_source("pg-3")],
        ),
    );
    cluster.fail_next_write_to("fabric-runtime-tenants");
    let error = writer.publish(&next).await.unwrap_err();
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
    assert_eq!(
        writer.current().await.unwrap(),
        PublishedRevisions {
            tenants: Some(DocumentRevision::new(1)),
            data_sources: Some(DocumentRevision::new(2)),
            catalog: Some(DocumentRevision::new(1)),
        }
    );

    let report = writer.publish(&next).await.unwrap();
    assert_eq!(report.data_sources, DocumentOutcome::Unchanged);
    assert_eq!(report.tenants, DocumentOutcome::Written);
}
