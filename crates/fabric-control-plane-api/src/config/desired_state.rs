//! Which desired-state repository this deployment uses.

use fabric_client_git::GitRepositoryConfig;

/// Where client desired state lives.
///
/// # A tagged enum, so a development adapter can never be reached by accident
///
/// The modes are not interchangeable: one is established by an operator, one
/// is stated by the deployment, and the third forgets everything on restart. A
/// boolean or an optional Git section would let a deployment fall into the
/// last by omission — which is the failure where production quietly serves an
/// empty client list and nobody can tell it apart from a platform with no
/// clients.
///
/// Naming the mode makes that impossible to do without writing it down.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum DesiredStateConfig {
    /// Established in the product by an operator, not by this file.
    ///
    /// **The production mode.** The platform starts knowing only that it has
    /// no desired-state repository, reports itself unconfigured, and stays
    /// available so that an operator can connect one through the console. What
    /// they establish is durable, so a restart picks it up again.
    ///
    /// This exists because the alternative was a deployment holding a Git
    /// host's identifiers and a credential that a human had to create by hand
    /// before the platform could start at all — which made the platform's own
    /// onboarding somebody else's problem.
    Managed,

    /// A Git repository, reached over its hosting provider's contents API.
    ///
    /// Stated by the deployment rather than established in the product. Kept
    /// because a repository whose location is genuinely fixed is a legitimate
    /// thing to state, and because it is what the integration tests drive —
    /// but a deployment that states it is opting out of the operator-managed
    /// path, and its configuration is fatal if wrong, exactly as before.
    ///
    /// Boxed because it is much larger than the others — a repository
    /// location plus an authentication posture, against one path — and an
    /// unboxed enum is as big as its largest variant everywhere it is moved.
    /// Nothing here is on a hot path; the box is for the type's size, not its
    /// speed.
    Git(Box<GitRepositoryConfig>),

    /// A persistent development store, opened from a local directory.
    ///
    /// **Development only** — the host logs a warning saying so at startup
    /// rather than leaving it to be discovered — but not in-memory. The
    /// top-level `*.yaml` files (one client document each) are imported the
    /// first time the directory is opened; from then on `.fabric-state.json`
    /// is authoritative, and every write commits to it before this call
    /// returns, so a restart picks up exactly where the process left off. An
    /// OS-level lock on the directory refuses a second process opening it,
    /// so two processes cannot each believe they hold the only copy.
    ///
    /// It exists because the control plane must be runnable without a cluster
    /// (§22), and because a fake that skipped optimistic concurrency would
    /// make every test of the conflict path meaningless — this one keeps it.
    /// See [`LocalClientRepository`](crate::local_repository::LocalClientRepository)
    /// for the storage format.
    LocalDirectory {
        /// The directory holding the imported `*.yaml` documents and, once
        /// anything has been written, `.fabric-state.json`.
        path: std::path::PathBuf,
    },
}
