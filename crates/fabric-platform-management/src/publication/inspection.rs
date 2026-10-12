//! The inspection that ends every publication pass (G3).

use fabric_runtime_publication::PublicationCompleteness;

use super::RuntimePublisher;

impl RuntimePublisher {
    /// Inspects the target once the pass has done all it will, whatever it
    /// decided: after an accepted offer, after an `Unwritable` that outlived
    /// its immediate re-offers, and after a pass that offered nothing.
    /// The panel only ever renders what this recorded, so a `GET` never
    /// reads the target.
    pub(super) async fn inspect_target(&self) -> PublicationCompleteness {
        let completeness = self.target.inspect().await;
        if !matches!(completeness, PublicationCompleteness::Complete { .. }) {
            tracing::warn!(
                event = "control_plane.publication.incomplete",
                environment = %self.environment,
                target = self.target.describe(),
                completeness = ?completeness,
                "the publication target does not hold one whole recorded generation"
            );
        }
        completeness
    }
}
