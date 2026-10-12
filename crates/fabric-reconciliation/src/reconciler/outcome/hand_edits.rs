//! What a pass did with unsupported hand edits (D01-19b).

use crate::provider::HandEdit;
use crate::ReconciliationOutcome;

impl ReconciliationOutcome {
    /// The same outcome, having left these hand edits in place.
    #[must_use]
    pub fn holding(mut self, hand_edits: Vec<HandEdit>) -> Self {
        self.hand_edits = hand_edits;
        self
    }

    /// The same outcome, having overwritten these hand edits at an
    /// operator's explicit request, each confirmed by the provider.
    #[must_use]
    pub fn overwriting(mut self, overwritten: Vec<HandEdit>) -> Self {
        self.overwritten = overwritten;
        self
    }

    /// The same outcome, with these hand edits attempted by a failing call
    /// whose effect is unknown.
    #[must_use]
    pub fn unconfirmed(mut self, unconfirmed: Vec<HandEdit>) -> Self {
        self.unconfirmed = unconfirmed;
        self
    }

    /// The unsupported hand edits this pass left in place.
    #[must_use]
    pub fn hand_edits(&self) -> &[HandEdit] {
        &self.hand_edits
    }

    /// The unsupported hand edits this pass overwrote, as confirmed by the
    /// provider. Only a re-assertion overwrites; on a failed one, these are
    /// the overwrites that completed before the failing call.
    #[must_use]
    pub fn overwritten(&self) -> &[HandEdit] {
        &self.overwritten
    }

    /// The unsupported hand edits a failing re-assertion call attempted,
    /// which may or may not have been overwritten. Never claimed as either.
    #[must_use]
    pub fn unconfirmed_overwrites(&self) -> &[HandEdit] {
        &self.unconfirmed
    }
}
