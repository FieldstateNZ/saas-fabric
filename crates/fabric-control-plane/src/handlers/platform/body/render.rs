//! Turning what the rules know into what the console reads.

use fabric_platform_management::{
    CheckOutcome, ComponentStatus, DesiredStateStatus, LastCheck, Running, UpdatePolicy,
};

use super::{ComponentRow, DiagnosticRow, HoldRow, LastCheckRow, PlatformBody};

impl PlatformBody {
    /// Renders an environment's statuses.
    pub(in crate::handlers::platform) fn of(
        environment: &str,
        components: &[ComponentStatus],
        last: Option<&LastCheck>,
    ) -> Self {
        Self {
            environment: environment.to_owned(),
            components: components.iter().map(ComponentRow::of).collect(),
            last_check: last.map(LastCheckRow::of),
        }
    }
}

impl ComponentRow {
    /// Renders one component.
    fn of(status: &ComponentStatus) -> Self {
        let mut diagnostics: Vec<DiagnosticRow> = status
            .diagnostics
            .not_yet
            .iter()
            .map(|version| DiagnosticRow {
                version: version.as_str().to_owned(),
                state: "publishing",
            })
            .collect();

        diagnostics.extend(status.diagnostics.incoherent.iter().map(|version| DiagnosticRow {
            version: version.as_str().to_owned(),
            state: "incoherent",
        }));

        Self {
            component: status.component.clone(),
            desired: status.desired.as_str().to_owned(),
            available: status
                .available
                .as_ref()
                .map(|version| version.as_str().to_owned()),
            running: match status.running {
                Running::Unknown => "unknown",
            },
            policy: match status.policy {
                UpdatePolicy::Automatic => "automatic",
                UpdatePolicy::Manual => "manual",
                UpdatePolicy::Locked => "locked",
            },
            paused: status.is_paused(),
            desired_state: match status.desired_state {
                DesiredStateStatus::Current => "current",
                DesiredStateStatus::UpdateAvailable => "update-available",
            },
            hold: status.hold.as_ref().map(|hold| HoldRow {
                reason: hold.reason.clone(),
                since: hold.since.clone(),
                note: hold.note.clone(),
            }),
            diagnostics,
        }
    }
}

impl LastCheckRow {
    /// Renders the last sweep.
    fn of(last: &LastCheck) -> Self {
        match &last.outcome {
            CheckOutcome::Succeeded => Self {
                at_unix_seconds: last.at_unix_seconds,
                outcome: "success",
                detail: None,
            },
            CheckOutcome::Failed { detail } => Self {
                at_unix_seconds: last.at_unix_seconds,
                outcome: "failure",
                detail: Some(detail.as_str().to_owned()),
            },
        }
    }
}
