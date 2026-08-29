//! Where this Fabric instance keeps what it must not lose.

use fabric_openbao::OpenBaoConfig;

/// The instance's secret partition.
///
/// # A tagged enum, so a development store can never be reached by accident
///
/// The same reasoning as the desired-state modes. One of these keeps a GitHub
/// App's private key durably; the other forgets it when the process stops. A
/// deployment that fell into the second by omission would connect an
/// integration that silently stopped working at the next restart, and the
/// symptom would appear nowhere near the cause.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SecretStoreConfig {
    /// OpenBao, reached with the pod's own Kubernetes identity.
    ///
    /// The production mode.
    OpenBao(OpenBaoConfig),

    /// Held in this process and lost when it stops.
    ///
    /// **Development only.** The control plane must be runnable without a
    /// cluster (§22), and connecting an integration is part of what a
    /// developer needs to exercise.
    InMemory,
}

impl Default for SecretStoreConfig {
    /// In-memory, because the only deployment with no `[secret_store]` section
    /// is a local one. A production deployment states OpenBao, and a
    /// production deployment that forgets to is told at startup that its
    /// integration will not survive a restart.
    fn default() -> Self {
        Self::InMemory
    }
}
