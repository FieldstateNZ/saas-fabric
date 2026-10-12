//! A client with one projected identity client, built through the real
//! router: `analytics` published, `newco` created assigned to it.

// Reading a JSON field is the assertion here; see `mod.rs`.
#![allow(clippy::indexing_slicing)]

use axum::body::Body;
use axum::Router;
use http::{header, StatusCode};
use serde_json::{json, Value};

use fabric_client_model::{ClientDocument, ClientId};
use fabric_control_plane::{ChangeContext, ClientRepository as _};

use super::{as_operator, entity_tag, json as body_json, send, TestControlPlane};

/// The analytics definition, with the hostname template `{client}.example.com`.
pub fn analytics_definition() -> Value {
    json!({
        "name":"Analytics", "description":"Reports", "domain":"{client}.example.com",
        "components":[{"id":"reports","name":"Reports","kind":"container","reference":"registry.example.com/reports","version":"1.0.0","required":false,"policy":"manual"}],
        "features":[{"id":"reporting","name":"Reporting","description":"","implementedBy":["reports"]}],
        "plans":[{"id":"standard","name":"Standard","description":"","features":["reporting"],"configuration":{}}],
        "fields":[], "navigation":[]})
}

/// Runs catalogue commands in order and returns the catalogue's revision.
pub async fn catalogue_commands(router: &Router, mut revision: Value, commands: Vec<Value>) -> Value {
    for command in commands {
        let builder = as_operator("POST", "/api/catalogue").header(header::CONTENT_TYPE, "application/json");
        let builder = match revision.as_str() {
            Some(revision) => builder.header(header::IF_MATCH, format!("\"{revision}\"")),
            None => builder.header(header::IF_NONE_MATCH, "*"),
        };
        let response = send(router, builder.body(Body::from(command.to_string())).unwrap()).await;
        let status = response.status();
        let body = body_json(response).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        revision = body["revision"].clone();
    }
    revision
}

/// Publishes `analytics` and creates `newco` assigned to it, so `newco`'s
/// identity holds one projected client.
pub async fn newco_with_a_projected_client(router: &Router) -> String {
    newco_on_hosts(router, &["newco.example.com"], None).await
}

/// As [`newco_with_a_projected_client`], with `newco` declaring `hosts`
/// and, when given, the scheme its `.internal` callbacks use.
pub async fn newco_on_hosts(router: &Router, hosts: &[&str], scheme: Option<&str>) -> String {
    catalogue_commands(
        router,
        Value::Null,
        vec![
            json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
            json!({"action":"saveApplication","id":"analytics","definition":analytics_definition()}),
            json!({"action":"publishApplication","id":"analytics","note":"Initial release"}),
        ],
    )
    .await;

    let created = send(
        router,
        as_operator("POST", "/api/clients")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":{
                    "displayName":"Newco","legalName":"Newco Ltd","region":"NZ","timezone":"Pacific/Auckland",
                    "hosts":hosts,"privateNetworkScheme":scheme,"configuration":{},
                    "applications":[{"applicationId":"analytics","version":1,"planId":"standard","configuration":{}}]}})
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    let identity = send(
        router,
        as_operator("GET", "/api/clients/newco/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    entity_tag(&identity)
}

/// Edits `newco`'s stored document as text, the way a commit to the
/// configuration repository would: rendered, edited, and parsed by
/// `ClientDocument::parse`, the parser every repository read goes through.
/// Returns the revision it produced.
pub async fn hand_edit_in_git(plane: &TestControlPlane, edit: impl FnOnce(String) -> String) -> String {
    let newco = ClientId::try_new("newco").unwrap();
    let stored = plane.repository.get(&newco).await.unwrap();
    let text = edit(stored.document.render().unwrap());
    let edited = ClientDocument::parse(&text).expect("the hand-edited text must still parse");
    let change = ChangeContext {
        requested_by: "someone-in-git".into(),
        summary: "hand edit".into(),
    };
    plane
        .repository
        .update(&newco, &edited, &stored.revision, &change)
        .await
        .unwrap()
        .as_str()
        .to_owned()
}
