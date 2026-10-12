//! What each of the two bodies contains — and, for the minimal one, what it
//! must never contain.

use super::connector_health::{ConnectorHealth, ConnectorOutcome};
use super::readiness_body::{detailed, minimal};
use super::readiness_facts::RegistryFacts;
use fabric_tenant_runtime::{GenerationOrigin, LastKnownGoodState, ReaderReport, ReaderStatus, Serving};

fn outcome(id: &str, health: ConnectorHealth) -> ConnectorOutcome {
    ConnectorOutcome {
        id: id.to_owned(),
        health,
    }
}

fn holding(count: usize) -> RegistryFacts {
    RegistryFacts { primed: true, count }
}

#[test]
fn the_minimal_body_carries_the_verdict_and_nothing_else() {
    let body = minimal(false);

    let keys: Vec<&String> = body.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["ready"]);
    assert_eq!(body["ready"], serde_json::Value::Bool(false));
}

#[test]
fn the_minimal_body_names_no_connector_and_no_count() {
    // The whole point of the split: this is what an unauthenticated caller on
    // the application-facing port gets, however broken the estate is.
    let rendered = minimal(false).to_string();

    assert!(!rendered.contains("postgres"));
    assert!(!rendered.contains("connectors"));
    assert!(!rendered.contains("tenants"));
}

#[test]
fn the_detailed_body_carries_a_reason_only_where_there_is_one() {
    let outcomes = vec![
        outcome("analytics", ConnectorHealth::Healthy),
        outcome(
            "postgres-au-east",
            ConnectorHealth::Unhealthy("shard 3 is gone".to_owned()),
        ),
        outcome("sqlserver-primary", ConnectorHealth::Unknown),
    ];

    let body = detailed(
        false,
        true,
        (&holding(3), &holding(4)),
        &ReaderStatus::default().report(),
        &outcomes,
    );

    assert_eq!(body["tenants"], serde_json::Value::from(3));
    assert_eq!(body["data_sources"], serde_json::Value::from(4));
    assert_eq!(body["degraded"], serde_json::Value::Bool(true));

    let connectors = body["connectors"].as_array().unwrap();
    assert_eq!(connectors[0]["status"], serde_json::Value::from("healthy"));
    assert!(connectors[0].get("reason").is_none());

    assert_eq!(connectors[1]["status"], serde_json::Value::from("unhealthy"));
    assert_eq!(
        connectors[1]["reason"],
        serde_json::Value::from("shard 3 is gone")
    );

    // An unknown connector has no diagnosis to give. Inventing one would dress
    // a timeout up as an answer.
    assert_eq!(connectors[2]["status"], serde_json::Value::from("unknown"));
    assert!(connectors[2].get("reason").is_none());
}

#[test]
fn the_detailed_body_says_which_generation_is_served_from_where_and_why_it_is_held() {
    let publication = ReaderReport {
        serving: Serving::Generation {
            generation: 7,
            origin: GenerationOrigin::LastKnownGood {
                held_because: "no publication record".to_owned(),
            },
        },
        held_passes: 2,
        last_hold: Some("tenants is not the one generation 8 names".to_owned()),
        last_known_good: LastKnownGoodState::Stored { generation: 7 },
    };

    let body = detailed(true, true, (&holding(1), &holding(1)), &publication, &[]);

    assert_eq!(
        body["publication"],
        serde_json::json!({
            "serving": "generation",
            "generation": 7,
            "origin": "last_known_good",
            "held_because": "no publication record",
            "held_passes": 2,
            "last_hold": "tenants is not the one generation 8 names",
            "last_known_good": { "state": "stored", "generation": 7 },
        })
    );
}

#[test]
fn the_minimal_body_says_nothing_about_the_publication() {
    assert!(!minimal(true).to_string().contains("generation"));
}
