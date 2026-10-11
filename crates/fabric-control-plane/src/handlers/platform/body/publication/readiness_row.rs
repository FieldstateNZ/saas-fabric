//! The readiness row: the three gates the runtime is raised against
//! (D01-18), as Fabric evidences them.

use fabric_platform_management::{GateStatus, ReadinessReport};

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
    /// Published tenants no issuer reaches, and why.
    pub withheld: Vec<WithheldRow>,
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

/// A published tenant left out of the issuer registry.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithheldRow {
    /// The tenant.
    pub tenant: String,
    /// Why, in an operator's words.
    pub reason: String,
}

impl From<ReadinessReport> for ReadinessRow {
    fn from(report: ReadinessReport) -> Self {
        Self {
            documents: report.documents.into(),
            identity: report.identity.into(),
            connectors: report.connectors.into(),
            withheld: report
                .withheld
                .into_iter()
                .map(|withheld| WithheldRow {
                    tenant: withheld.tenant.to_string(),
                    reason: withheld.reason,
                })
                .collect(),
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
