//! O5: what a Git revert of the product catalogue does through the real
//! router, the controller's pass and a real filesystem publication target
//! (D01-27: restoring the catalogue must keep conditional writes and audit
//! evidence, and any break-glass Git effect is characterized here before it
//! is claimed as a supported rollback).
//!
//! A revert is stood in for by writing the catalogue as it stood before the
//! reverted command straight to the repository, beside the console: that is
//! what `fabric-catalogue.yaml` holds after `git revert`, and it reaches the
//! control plane as an ordinary read. It passes through no console rule and
//! records nothing in the audit trail, as a commit does not.
//!
//! The in-memory repository's revisions are a write counter, so here a
//! revert is always a new revision. The Git adapter names a revision by its
//! blob, so a revert to identical bytes brings the earlier one back; that is
//! characterized in `fabric-client-git`'s `desired_state_repository.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::Router;
use fabric_client_model::catalogue::Catalogue;
use fabric_control_plane::{AuditStream, AuditTrail as _, ChangeContext, ClientRepository, PublicationSink};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_platform_management::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDeclaration, DesiredRevision, IsolationModelDocument,
    PlacementClassDocument, PlacementRecord, PoolSettingsDocument,
};
use fabric_runtime_publication::FilesystemRuntimePublication;
use http::{header, StatusCode};
use serde_json::{json, Value};
use support::platform_fixture::platform_binding;
use support::{as_operator, control_plane_with_publication, json as body_of, send, TempDir};

const DATA_SOURCE: &str = "shared-postgres-nz-01";

fn sink(dir: &TempDir) -> PublicationSink {
    PublicationSink {
        target: Arc::new(FilesystemRuntimePublication::new(
            dir.path().join("tenants.json"),
            dir.path().join("data-sources.json"),
            dir.path().join("catalog.json"),
        )),
        issuer_origin: None,
        readers: None,
    }
}

fn data_source() -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(DATA_SOURCE).unwrap(),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres-nz").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").unwrap(),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: Some("NZ".to_owned()),
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument {
            writable: true,
            accepts_new_tenants: true,
        },
        discriminator: Some(fabric_platform_management::Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").unwrap(),
        }),
        labels: BTreeMap::new(),
    }
}

fn placement() -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new("acme").unwrap(),
        logical: LogicalDataSourceName::try_new("primary").unwrap(),
        revision: BindingRevision::new(1),
        data_source: DataSourceId::try_new(DATA_SOURCE).unwrap(),
        isolation: IsolationModelDocument::Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").unwrap(),
            value: "acme-key".to_owned(),
        },
        placed_at: "2026-10-12T00:00:00Z".to_owned(),
    }
}

/// `workspec`'s definition, declaring one resource per name on `primary`.
fn definition(resources: &[&str]) -> Value {
    let resources: Vec<Value> = resources
        .iter()
        .map(|name| {
            json!({
                "name": name, "dataSource": "primary", "collection": name, "keyField": "id",
                "operations": ["read", "list"], "queryableFields": [],
            })
        })
        .collect();
    json!({
        "name": "Workspec", "description": "", "domain": "{client}.example.com",
        "components": [], "features": [],
        "plans": [{"id":"standard","name":"Standard","description":"","features":[],"configuration":{}}],
        "fields": [], "navigation": [], "resources": resources,
    })
}

/// Sends one catalogue command at `revision` (`None` for the first write).
async fn command(router: &Router, revision: Option<&str>, body: &Value) -> (StatusCode, Value) {
    let builder = as_operator("POST", "/api/catalogue").header(header::CONTENT_TYPE, "application/json");
    let builder = match revision {
        Some(revision) => builder.header(header::IF_MATCH, format!("\"{revision}\"")),
        None => builder.header(header::IF_NONE_MATCH, "*"),
    };
    let response = send(router, builder.body(Body::from(body.to_string())).unwrap()).await;
    let status = response.status();
    (status, body_of(response).await)
}

/// Sends one catalogue command that must be accepted, and returns the new
/// revision.
async fn accepted(router: &Router, revision: Option<&str>, body: &Value) -> String {
    let (status, body) = command(router, revision, body).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["revision"].as_str().unwrap().to_owned()
}

async fn assign(router: &Router, client: &str, version: u64) -> Value {
    let response = send(
        router,
        as_operator("POST", "/api/clients")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"id": client, "configuration": {
                    "displayName": client, "legalName": "Ltd", "region": "NZ", "timezone": "Pacific/Auckland",
                    "hosts": [format!("{client}.example.com")], "configuration": {},
                    "applications": [{"applicationId":"workspec","version":version,"planId":"standard","configuration":{}}]}})
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

async fn get(router: &Router, path: &str) -> Value {
    let response = send(router, as_operator("GET", path).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    body_of(response).await
}

/// Runs one pass through `POST /api/platform/publication` and returns its
/// `lastPass` row.
async fn pass(router: &Router) -> Value {
    let response = send(
        router,
        as_operator("POST", "/api/platform/publication")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    body_of(response).await["lastPass"].clone()
}

fn read(dir: &TempDir, name: &str) -> Vec<u8> {
    std::fs::read(dir.path().join(name)).unwrap()
}

fn document(dir: &TempDir, name: &str) -> Value {
    serde_json::from_slice(&read(dir, name)).unwrap()
}

/// The resource names a released definition declares.
fn resources(definition: &Value) -> Vec<&str> {
    definition["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|resource| resource["name"].as_str().unwrap())
        .collect()
}

fn git_revert() -> ChangeContext {
    ChangeContext {
        requested_by: "someone-in-git".to_owned(),
        summary: "Revert \"publish workspec 2\"".to_owned(),
    }
}

struct Environment {
    dir: TempDir,
    plane: support::TestControlPlane,
    /// The catalogue as it stood before release 2 was published: what a
    /// revert of that commit restores.
    before_release_2: Catalogue,
    /// The revision release 2's publication left, which an operator's
    /// console still holds as its tag.
    release_2_revision: String,
}

/// `workspec` released twice through the console -- 1 with `customers`, 2
/// adding `invoices` -- `newco` assigned release 2, and a pass that has
/// published both resources for `acme`, placed on `primary`.
async fn released_twice(label: &str) -> Environment {
    let dir = TempDir::new(label);
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    fake.seed(DesiredRevision::new("data-sources-1"), vec![data_source()]);
    fake.seed_placements(DesiredRevision::new("placements-1"), vec![placement()]);

    let router = &plane.router;
    let revision = accepted(
        router,
        None,
        &json!({"action":"createApplication","id":"workspec","name":"Workspec"}),
    )
    .await;
    let revision = accepted(
        router,
        Some(&revision),
        &json!({"action":"saveApplication","id":"workspec","definition":definition(&["customers"])}),
    )
    .await;
    let revision = accepted(
        router,
        Some(&revision),
        &json!({"action":"publishApplication","id":"workspec","note":"1"}),
    )
    .await;
    accepted(
        router,
        Some(&revision),
        &json!({"action":"saveApplication","id":"workspec","definition":definition(&["customers", "invoices"])}),
    )
    .await;
    let before_release_2 = plane.repository.catalogue().await.unwrap();
    let release_2_revision = accepted(
        router,
        before_release_2
            .revision
            .as_ref()
            .map(fabric_client_model::ClientRevision::as_str),
        &json!({"action":"publishApplication","id":"workspec","note":"2"}),
    )
    .await;
    assign(router, "newco", 2).await;

    let row = pass(router).await;
    assert_eq!(row["outcome"], "published", "{row}");
    let catalog = document(&dir, "catalog.json");
    assert!(catalog.get("invoices").is_some(), "{catalog}");

    Environment {
        dir,
        plane,
        before_release_2: before_release_2.catalogue,
        release_2_revision,
    }
}

impl Environment {
    /// `git revert` of release 2's publication, landing as one commit.
    async fn revert_release_2(&self) {
        let stored = self.plane.repository.catalogue().await.unwrap();
        self.plane
            .repository
            .save_catalogue(&self.before_release_2, stored.revision.as_ref(), &git_revert())
            .await
            .unwrap();
    }

    async fn current_revision(&self) -> String {
        self.plane
            .repository
            .catalogue()
            .await
            .unwrap()
            .revision
            .unwrap()
            .as_str()
            .to_owned()
    }
}

#[tokio::test]
async fn reverting_a_release_publishes_the_older_definition_forward_at_a_new_catalogue_revision() {
    let env = released_twice("catalogue-revert-publishes").await;
    let router = &env.plane.router;
    let held = document(&env.dir, "publication.json");
    let tenants = read(&env.dir, "tenants.json");
    let data_sources = read(&env.dir, "data-sources.json");

    env.revert_release_2().await;

    // Nothing holds a catalogue entry to what was published: the pass
    // offers the derived catalogue as it now reads, and the publisher takes
    // it, because only the document's bytes are compared. `invoices` leaves
    // the runtime catalogue. The document's revision still moves forward,
    // never back to the one that last held this content.
    let row = pass(router).await;
    assert_eq!(row["outcome"], "published", "{row}");
    let catalog = document(&env.dir, "catalog.json");
    assert!(catalog.get("customers").is_some(), "{catalog}");
    assert!(catalog.get("invoices").is_none(), "{catalog}");
    let record = document(&env.dir, "publication.json");
    let before = held["documents"]["catalog"]["revision"].as_u64().unwrap();
    assert_eq!(
        record["documents"]["catalog"]["revision"].as_u64().unwrap(),
        before + 1,
        "{record}"
    );
    assert_eq!(read(&env.dir, "tenants.json"), tenants);
    assert_eq!(read(&env.dir, "data-sources.json"), data_sources);

    // The console reads the reverted catalogue as it is: release 2 is gone
    // from it, and the draft that release published is back in place.
    let stored = get(router, "/api/catalogue").await;
    let workspec = &stored["catalogue"]["applications"][0];
    assert_eq!(workspec["releases"].as_array().unwrap().len(), 1, "{workspec}");
    assert_eq!(resources(&workspec["draft"]), ["customers", "invoices"]);

    // A client assigned release 2 keeps its own copy of it: the revert does
    // not reach client documents.
    let product = get(router, "/api/clients/newco/product").await;
    let assignment = &product["product"]["applications"][0];
    assert_eq!(assignment["release"]["version"], 2, "{product}");
    assert_eq!(
        resources(&assignment["release"]["definition"]),
        ["customers", "invoices"]
    );
}

#[tokio::test]
async fn a_revert_that_leaves_no_release_waits_and_the_last_catalogue_stays_published() {
    let env = released_twice("catalogue-revert-empty").await;
    let router = &env.plane.router;
    let before: Vec<Vec<u8>> = [
        "tenants.json",
        "data-sources.json",
        "catalog.json",
        "publication.json",
    ]
    .iter()
    .map(|name| read(&env.dir, name))
    .collect();

    // Reverting every catalogue commit back to one with no release in it.
    let stored = env.plane.repository.catalogue().await.unwrap();
    env.plane
        .repository
        .save_catalogue(&Catalogue::default(), stored.revision.as_ref(), &git_revert())
        .await
        .unwrap();

    // An empty derived catalogue is not offered at all: the pass waits, and
    // every published file stays what it was.
    let row = pass(router).await;
    assert_eq!(row["outcome"], "waiting", "{row}");
    let after: Vec<Vec<u8>> = [
        "tenants.json",
        "data-sources.json",
        "catalog.json",
        "publication.json",
    ]
    .iter()
    .map(|name| read(&env.dir, name))
    .collect();
    assert_eq!(after, before);
}

#[tokio::test]
async fn a_catalogue_revert_leaves_no_audit_record_and_drops_its_command_from_the_activity_view() {
    let env = released_twice("catalogue-revert-audit").await;
    let router = &env.plane.router;
    let audited = env.plane.audit.read(&AuditStream::Catalogue).await.unwrap();
    assert_eq!(audited.entries.len(), 5);
    let releases_shown = |activity: &Value| {
        activity["activity"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["action"] == "Application definition published")
            .count()
    };
    assert_eq!(releases_shown(&get(router, "/api/activity").await), 2);

    env.revert_release_2().await;

    // The trail still holds release 2's publication, at the revision the
    // revert replaced, and nothing for the revert itself.
    let after = env.plane.audit.read(&AuditStream::Catalogue).await.unwrap();
    assert_eq!(after.entries.len(), 5);
    assert_eq!(after.verify().unwrap().map(|head| head.sequence), Some(5));
    let last = after.entries.last().unwrap().record();
    assert_eq!(last.field("revision"), Some(env.release_2_revision.as_str()));
    assert_ne!(env.current_revision().await, env.release_2_revision);

    // The activity view is read from the catalogue document, so the revert
    // takes release 2's publication out of it.
    assert_eq!(releases_shown(&get(router, "/api/activity").await), 1);
}

#[tokio::test]
async fn after_a_revert_a_console_tag_from_before_it_is_refused_and_the_restore_is_conditional_and_audited() {
    let env = released_twice("catalogue-revert-restore").await;
    let router = &env.plane.router;
    let published = document(&env.dir, "publication.json")["documents"]["catalog"]["revision"]
        .as_u64()
        .unwrap();
    env.revert_release_2().await;
    assert_eq!(pass(router).await["outcome"], "published");

    // An operator whose console still holds the tag release 2 left is
    // refused, and nothing is written.
    let reverted = env.current_revision().await;
    let (status, body) = command(
        router,
        Some(&env.release_2_revision),
        &json!({"action":"publishApplication","id":"workspec","note":"restore"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(env.current_revision().await, reverted);

    // Read again, the restore is an ordinary conditional command, audited
    // at the revision it wrote. The runtime catalogue takes `invoices`
    // back at a revision above both earlier ones.
    let restored = accepted(
        router,
        Some(&reverted),
        &json!({"action":"publishApplication","id":"workspec","note":"restore"}),
    )
    .await;
    let trail = env.plane.audit.read(&AuditStream::Catalogue).await.unwrap();
    assert_eq!(trail.entries.len(), 6);
    let last = trail.entries.last().unwrap().record();
    assert_eq!(last.event(), "control_plane.audit.catalogue_changed");
    assert_eq!(last.field("revision"), Some(restored.as_str()));

    assert_eq!(pass(router).await["outcome"], "published");
    assert!(document(&env.dir, "catalog.json").get("invoices").is_some());
    assert_eq!(
        document(&env.dir, "publication.json")["documents"]["catalog"]["revision"]
            .as_u64()
            .unwrap(),
        published + 2
    );
}

/// Characterizes a break in release immutability: a revert frees release
/// 2's number, the console's next publication takes it again for another
/// definition, and "workspec 2" then names two definitions at once.
#[tokio::test]
async fn a_release_number_freed_by_a_revert_is_issued_again_to_a_different_definition() {
    let env = released_twice("catalogue-revert-renumber").await;
    let router = &env.plane.router;
    env.revert_release_2().await;

    let revision = accepted(
        router,
        Some(&env.current_revision().await),
        &json!({"action":"saveApplication","id":"workspec","definition":definition(&["customers", "payments"])}),
    )
    .await;
    accepted(
        router,
        Some(&revision),
        &json!({"action":"publishApplication","id":"workspec","note":"2 again"}),
    )
    .await;

    let stored = get(router, "/api/catalogue").await;
    let releases = stored["catalogue"]["applications"][0]["releases"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(releases.len(), 2);
    assert_eq!(releases[1]["version"], 2);
    assert_eq!(resources(&releases[1]["definition"]), ["customers", "payments"]);

    // `newco` was assigned the first release 2 and keeps it, through a save
    // that names version 2 too: an unchanged assignment keeps the client's
    // own copy, matched by number alone.
    let product = get(router, "/api/clients/newco/product").await;
    let tag = product["client"]["revision"].as_str().unwrap().to_owned();
    let response = send(
        router,
        as_operator("PUT", "/api/clients/newco/product")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{tag}\""))
            .body(Body::from(
                json!({"displayName":"Newco","legalName":"Newco Ltd","region":"NZ","timezone":"Pacific/Auckland",
                    "hosts":["newco.example.com"],"configuration":{},
                    "applications":[{"applicationId":"workspec","version":2,"planId":"standard","configuration":{}}]})
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let saved = get(router, "/api/clients/newco/product").await;
    let kept = &saved["product"]["applications"][0]["release"];
    assert_eq!(kept["version"], 2);
    assert_eq!(resources(&kept["definition"]), ["customers", "invoices"]);

    // A client assigned version 2 now gets the other definition.
    assign(router, "initech", 2).await;
    let other = get(router, "/api/clients/initech/product").await;
    let issued = &other["product"]["applications"][0]["release"];
    assert_eq!(issued["version"], 2);
    assert_eq!(resources(&issued["definition"]), ["customers", "payments"]);

    // The runtime catalogue follows the catalogue's newest release, not
    // what any client was assigned.
    assert_eq!(pass(router).await["outcome"], "published");
    let catalog = document(&env.dir, "catalog.json");
    assert!(catalog.get("payments").is_some(), "{catalog}");
    assert!(catalog.get("invoices").is_none(), "{catalog}");
}
