//! The readiness row: the three gates the runtime is raised against
//! (D01-18), as Fabric evidences them, and what the generated issuer
//! registry leaves out.

use fabric_platform_management::{GateStatus, ReadinessReport, RegistryApplication};

/// The three gates, named as the platform contract names them. Publisher
/// evidence only: an operator still raises replicas deliberately, and the
/// platform's own check decides what the edge and the runtime applied.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessRow {
    /// All three runtime documents published.
    pub documents: GateRow,
    /// An issuer registry generation published, naming at least one issuer.
    pub identity: GateRow,
    /// Never met by Fabric: platform composition deploys the connectors.
    pub connectors: GateRow,
    /// Bound tenants the generated registry leaves out, and why. Exclusion
    /// from what Fabric publishes -- not route withdrawal or access
    /// revocation: see `registry_application`.
    pub excluded_from_registry: Vec<ExcludedRow>,
    /// Whether any edge or runtime is known to apply the registry.
    pub registry_application: ApplicationRow,
}

/// One gate.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GateRow {
    /// Whether Fabric's evidence shows it held.
    pub met: bool,
    /// Why not, when it did not. Empty when met.
    pub reasons: Vec<String>,
}

/// A bound tenant left out of the generated issuer registry.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludedRow {
    /// The tenant.
    pub tenant: String,
    /// `notReady` (including identity evidence pending reconciliation),
    /// `retired`, `noClient` or `invalidIssuer`.
    pub kind: &'static str,
    /// Why, in an operator's words.
    pub detail: String,
}

/// What Fabric knows about the registry being applied.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationRow {
    /// Always `unobserved`: Fabric observes no edge or runtime applying it.
    pub state: &'static str,
    /// What that means, in an operator's words.
    pub detail: &'static str,
}

impl From<ReadinessReport> for ReadinessRow {
    fn from(report: ReadinessReport) -> Self {
        Self {
            documents: report.documents.into(),
            identity: report.identity.into(),
            connectors: report.connectors.into(),
            excluded_from_registry: report
                .excluded
                .into_iter()
                .map(|excluded| ExcludedRow {
                    tenant: excluded.tenant.to_string(),
                    kind: excluded.reason.kind(),
                    detail: excluded.reason.to_string(),
                })
                .collect(),
            registry_application: report.application.into(),
        }
    }
}

impl From<RegistryApplication> for ApplicationRow {
    fn from(application: RegistryApplication) -> Self {
        match application {
            RegistryApplication::Unobserved => Self {
                state: "unobserved",
                detail: "no runtime reads this registry and no edge application is observed; a tenant left \
                         out of it may still be served by trust configured elsewhere",
            },
        }
    }
}

impl From<GateStatus> for GateRow {
    fn from(status: GateStatus) -> Self {
        match status {
            GateStatus::Met => Self {
                met: true,
                reasons: Vec::new(),
            },
            GateStatus::Unmet { reasons } => Self { met: false, reasons },
        }
    }
}
