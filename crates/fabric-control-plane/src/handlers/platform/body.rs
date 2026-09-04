//! The shape the console reads.

mod render;

/// An environment's composition, as the console shows it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformBody {
    /// Which environment.
    pub environment: String,

    /// One row per component.
    pub components: Vec<ComponentRow>,

    /// What the last sweep found, or `null` if none has run.
    ///
    /// `null` is a real answer and the console says so. "Nothing has checked
    /// yet" and "checked, and found nothing to do" are different, and an
    /// operator wondering why a published version has not appeared needs to
    /// know which.
    pub last_check: Option<LastCheckRow>,
}

/// One component.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentRow {
    /// Its name.
    pub component: String,

    /// What the environment is asked to run.
    pub desired: String,

    /// What Fabric would advance this component to, if anything.
    ///
    /// Rendered as "Newer version", and not as "Available": nothing observes
    /// whether the desired version is itself still published, so the broader
    /// word would be a claim the platform cannot support. `null` says there is
    /// nothing to advance to.
    pub newer: Option<String>,

    /// What is actually running. Always `unknown` until there is a
    /// reconciliation integration to ask.
    pub running: &'static str,

    /// The standing decision about advancement.
    pub policy: &'static str,

    /// What this component is published as: `oci` or `helm`.
    ///
    /// Not "whether it can be rolled back". Both kinds can — rolling back
    /// restores an older published version — and what differs is
    /// how much of the old release returns: an image rollback restores the
    /// exact bytes, a chart rollback restores the version, and a chart
    /// repository may have republished the bytes behind it. The console needs
    /// the kind so it can say which, which the `rollable` boolean this
    /// replaces had nowhere to say from.
    pub artifact: &'static str,

    /// Whether an operator has paused an otherwise automatic component.
    ///
    /// A separate field rather than a third policy value, because the operator
    /// did not change the policy and the console should not claim they did.
    pub paused: bool,

    /// Whether desired state has somewhere to go.
    pub desired_state: &'static str,

    /// Why advancement is paused, when it is.
    pub hold: Option<HoldRow>,

    /// Versions that exist and were not selected.
    pub diagnostics: Vec<DiagnosticRow>,
}

/// An operator's hold.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldRow {
    /// Why advancement stopped.
    pub reason: String,

    /// When, as an RFC 3339 timestamp.
    pub since: String,

    /// What the operator wanted the next person to know.
    pub note: Option<String>,
}

/// A version that exists and was not selected.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticRow {
    /// The version.
    pub version: String,

    /// `publishing` — some of its images are not there yet, and it is expected
    /// to become available on a later pass. Or `incoherent` — its images were
    /// built from different commits, and waiting will not fix that.
    pub state: &'static str,
}

/// The last sweep.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastCheckRow {
    /// When it finished, as seconds since the Unix epoch.
    ///
    /// Sent unformatted so the browser can render it in the operator's own
    /// timezone. A server formatting a timestamp for somebody whose timezone
    /// it does not know is a server guessing.
    pub at_unix_seconds: u64,

    /// `success` or `failure`.
    pub outcome: &'static str,

    /// What went wrong, when something did. Already sanitised.
    pub detail: Option<String>,
}
