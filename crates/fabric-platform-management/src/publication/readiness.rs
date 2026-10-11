//! The readiness report: the three gates the runtime is raised against
//! (D01-18, ADR 0023 part 5), as far as the publisher can evidence them.

#[cfg(test)]
#[path = "readiness_tests.rs"]
mod readiness_tests;

use std::collections::BTreeSet;

use fabric_runtime_publication::PublishedRevisions;

use super::issuer_registry_source::Withheld;
use super::readiness_evidence::{ReadinessEvidence, RegistryEvidenceSummary};

/// One gate's state. Absence of evidence is never `Met`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateStatus {
    /// Fabric's evidence shows the gate held.
    Met,
    /// It does not, or Fabric holds no evidence that it does.
    Unmet {
        /// Why, in an operator's words.
        reasons: Vec<String>,
    },
}

/// The three gates, named as the platform contract names them: documents,
/// identity, connectors. Publisher-side evidence only: none says a runtime
/// or an edge applied anything, and replicas stay a deliberate operator
/// action (D01-18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessReport {
    /// All three documents published.
    pub documents: GateStatus,
    /// An issuer registry generation published, naming at least one issuer.
    pub identity: GateStatus,
    /// Never met here: connectors are deployed and configured by platform
    /// composition, and Fabric observes neither. Lists what is required.
    pub connectors: GateStatus,
    /// Bound tenants no issuer reaches, and why. Informational: a client
    /// that is not ready, or is retired, is not routed, by design.
    pub withheld: Vec<Withheld>,
}

impl ReadinessReport {
    /// The report over `evidence`, or all three gates unmet without it.
    #[must_use]
    pub fn of(evidence: Option<&ReadinessEvidence>) -> Self {
        let Some(evidence) = evidence else {
            let none = || unmet("no publication pass has been accepted since the control plane started");
            return Self {
                documents: none(),
                identity: none(),
                connectors: none(),
                withheld: Vec::new(),
            };
        };
        Self {
            documents: documents(&evidence.revisions),
            identity: identity(evidence.registry.as_ref()),
            connectors: connectors(&evidence.connectors),
            withheld: evidence
                .registry
                .as_ref()
                .map(|registry| registry.withheld.clone())
                .unwrap_or_default(),
        }
    }
}

fn documents(revisions: &PublishedRevisions) -> GateStatus {
    let missing: Vec<String> = [
        ("tenants", revisions.tenants),
        ("data-sources", revisions.data_sources),
        ("catalog", revisions.catalog),
    ]
    .into_iter()
    .filter(|(_, revision)| revision.is_none())
    .map(|(document, _)| format!("{document} is not published"))
    .collect();
    if missing.is_empty() {
        GateStatus::Met
    } else {
        GateStatus::Unmet { reasons: missing }
    }
}

fn identity(registry: Option<&RegistryEvidenceSummary>) -> GateStatus {
    match registry {
        None => unmet("Fabric publishes no issuer registry: no identity issuer origin is configured"),
        Some(registry) if registry.registered.is_empty() => unmet(&format!(
            "issuer registry generation {} registers no issuer: no client is ready to route",
            registry.generation
        )),
        Some(_) => GateStatus::Met,
    }
}

fn connectors(connectors: &BTreeSet<String>) -> GateStatus {
    if connectors.is_empty() {
        return unmet("no published data source names a connector");
    }
    let named: Vec<&str> = connectors.iter().map(String::as_str).collect();
    unmet(&format!(
        "platform composition must deploy and configure {}; Fabric does not observe the runtime's connectors",
        named.join(", ")
    ))
}

fn unmet(reason: &str) -> GateStatus {
    GateStatus::Unmet {
        reasons: vec![reason.to_owned()],
    }
}
