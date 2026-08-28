//! Which desired-state repository this deployment uses.

use fabric_client_git::GitRepositoryConfig;

/// Where client desired state lives.
///
/// # A tagged enum, so a development adapter can never be reached by accident
///
/// The two modes are not interchangeable: one is the platform's source of
/// truth, and the other forgets everything on restart. A boolean or an
/// optional Git section would let a deployment fall into the second by
/// omission — which is the failure where production quietly serves an empty
/// client list and nobody can tell it apart from a platform with no clients.
///
/// Naming the mode makes that impossible to do without writing it down.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum DesiredStateConfig {
    /// A Git repository, reached over its hosting provider's contents API.
    ///
    /// The production mode, and the one ADR 0008 describes.
    ///
    /// Boxed because it is much larger than the other variant — a repository
    /// location plus an authentication posture, against one path — and an
    /// unboxed enum is as big as its largest variant everywhere it is moved.
    /// Nothing here is on a hot path; the box is for the type's size, not its
    /// speed.
    Git(Box<GitRepositoryConfig>),

    /// Documents loaded from a local directory into memory at startup.
    ///
    /// **Development only.** Writes are kept in memory and never reach the
    /// files, so a restart loses them. The host logs a warning saying so at
    /// startup rather than leaving it to be discovered.
    ///
    /// It exists because the control plane must be runnable without a cluster
    /// (§22), and because a fake that skipped optimistic concurrency would
    /// make every test of the conflict path meaningless — this one keeps it.
    LocalDirectory {
        /// The directory holding one `*.yaml` document per client.
        path: std::path::PathBuf,
    },
}
