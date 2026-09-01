//! What an operator is told, beside the status.

use crate::ControlPlaneError;

impl ControlPlaneError {
    /// The message the operator sees.
    ///
    /// Most errors say exactly what they mean — an operator is entitled to
    /// know what the platform is doing. The two that do not are the ones whose
    /// detail comes from outside: a repository failure's `detail` field never
    /// reaches here, and a stored-document failure reports the client and the
    /// validation rule but not the file it came from.
    #[must_use]
    pub fn public_message(&self) -> String {
        self.to_string()
    }
}
