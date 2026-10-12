//! What a pass withheld (D01-19b).

use fabric_client_model::OidcClientId;

use crate::ReconciliationOutcome;

impl ReconciliationOutcome {
    /// The same outcome, having left these declared clients unconverged at
    /// the caller's request.
    #[must_use]
    pub fn withholding(mut self, withheld: Vec<OidcClientId>) -> Self {
        self.withheld = withheld;
        self
    }

    /// The declared application clients this pass did not converge because
    /// the caller withheld them.
    #[must_use]
    pub fn withheld(&self) -> &[OidcClientId] {
        &self.withheld
    }
}
