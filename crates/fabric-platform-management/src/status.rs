//! What the console is told about a component.

use crate::{ComponentDesired, Discovery, Hold, UpdatePolicy, Version};

/// What is actually serving.
///
/// One variant, because one is all that can be answered honestly today.
/// Reporting a deployment as converged or updating requires the
/// reconciliation system, and Fabric knowing that Git changed is not the same
/// as knowing that a rollout started — a console that said "Updating" on the
/// strength of a commit would be reporting success from a Git write.
///
/// An enum rather than an `Option<Version>` so that gaining
/// `Converged`/`Degraded` later is an addition rather than a reinterpretation
/// of what `None` meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Running {
    /// Nothing here can say. There is no reconciliation integration yet.
    Unknown,
}

/// Whether desired state has somewhere to go.
///
/// Deliberately two values. Whether automation is *allowed* to act is a
/// separate question, answered by the policy and the hold — folding "paused"
/// in here would mix "there is a newer version" with "something may do
/// anything about it", and an operator needs both answers at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesiredStateStatus {
    /// Desired state is the newest eligible version.
    Current,

    /// Something newer is available.
    UpdateAvailable,
}

/// Versions that exist and were not selected, and why.
///
/// `not_yet` is transient — images still publishing — and is expected to
/// empty itself. `incoherent` is not: those versions were built more than
/// once, and no waiting fixes them. Reported so an environment jumping
/// `preview.2` to `preview.4` can say what happened to `preview.3`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diagnostics {
    /// Still publishing.
    pub not_yet: Vec<Version>,

    /// Built from more than one commit.
    pub incoherent: Vec<Version>,
}

/// Everything the console shows about one component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentStatus {
    /// Which component this is.
    pub component: String,

    /// What the environment is asked to run.
    pub desired: Version,

    /// What Fabric would advance to, if anything.
    ///
    /// The newest eligible version *newer than* [`desired`](Self::desired), and
    /// deliberately not "the available version": nothing here observes whether
    /// the desired version is still published. `None` answers "there is nothing
    /// to advance to", which is not the same claim as "nothing is available".
    pub newer: Option<Version>,

    /// What is actually serving.
    pub running: Running,

    /// The standing decision about advancement.
    pub policy: UpdatePolicy,

    /// Present while an operator has paused advancement.
    pub hold: Option<Hold>,

    /// Whether desired state has somewhere to go.
    pub desired_state: DesiredStateStatus,

    /// Versions that exist and were not selected.
    pub diagnostics: Diagnostics,
}

impl ComponentStatus {
    /// Whether automatic advancement is on but paused.
    ///
    /// The console shows this as `Automatic — Paused`, which is one fact about
    /// two fields rather than a third policy value: the operator did not
    /// change the policy, and the display should not claim they did.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.policy == UpdatePolicy::Automatic && self.hold.is_some()
    }

    /// Assembles what the console is told from desired state and a discovery.
    pub(crate) fn assemble(component: &str, desired: &ComponentDesired, discovery: &Discovery) -> Self {
        let newer = discovery.newer.as_ref().map(|unit| unit.version.clone());

        Self {
            component: component.to_owned(),
            desired: desired.version.clone(),
            // Discovery only considers versions above the desired one, so
            // anything it found at all is an upgrade.
            desired_state: if newer.is_some() {
                DesiredStateStatus::UpdateAvailable
            } else {
                DesiredStateStatus::Current
            },
            newer,
            running: Running::Unknown,
            policy: desired.policy,
            hold: desired.hold.clone(),
            diagnostics: Diagnostics {
                not_yet: discovery.not_yet.clone(),
                incoherent: discovery.incoherent.clone(),
            },
        }
    }
}

/// What one reconciliation did.
///
/// Carries where the component started as well as where it ended, because
/// "advanced" and "was already there" produce the same status and are not the
/// same event. A sweep reports one and stays quiet about the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciliation {
    /// The version desired before this ran.
    pub was: Version,

    /// The situation afterwards.
    pub status: ComponentStatus,
}

impl Reconciliation {
    /// Whether desired state moved.
    #[must_use]
    pub fn advanced(&self) -> bool {
        self.was != self.status.desired
    }
}
