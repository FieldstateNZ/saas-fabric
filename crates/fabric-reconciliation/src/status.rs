//! What a client's reconciliation currently looks like from outside.

mod report;
mod store;
mod transition;
#[cfg(test)]
mod transition_tests;

pub use report::ReconciliationReport;
pub use store::ReconciliationStatusStore;

/// Where a client's identity stands between what Git says and what the
/// provider holds.
///
/// # The five are not five flavours of the same thing
///
/// | Status | Means | Who fixes it |
/// |---|---|---|
/// | `Pending` | Desired state has changed and has not been reconciled since | Nobody — the next pass |
/// | `Applied` | The provider matches the desired state | — |
/// | `Failed` | The last pass could not converge it | Depends on the detail |
/// | `Drifted` | The provider had stopped matching a desired state already converged | Nobody, but somebody should know |
/// | `HandEdited` | The provider holds a hand edit the product does not support, left in place | An operator: remove the edit, or re-assert the projection |
///
/// `Drifted` is the one that is easy to leave out and expensive to lack.
/// Without it, an out-of-band change to a realm that reconciliation quietly
/// corrects looks exactly like an ordinary pass, so nobody ever learns that
/// something outside SaaS Fabric is editing the realms it owns.
///
/// `HandEdited` is `Drifted`'s counterpart for an edit the product will not
/// silently overwrite (D01-19b): a client switched off, its authorization-code
/// flow switched off, or a declared callback removed. Every product-owned
/// field is still converged; the report's `hand_edits` names what was held.
///
/// # `Pending` is why a successful write is not a success
///
/// Writing desired state to Git and making a provider match it are different
/// events that fail independently. The control plane answers a write with
/// `Pending` rather than `Applied` because at that moment the second event has
/// provably not happened yet (ADR 0008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReconciliationStatus {
    /// The desired state has not been reconciled since it last changed.
    Pending,

    /// The provider matches the desired state.
    Applied,

    /// The last pass could not converge the provider.
    Failed,

    /// The provider had diverged from a desired state that was already
    /// converged.
    Drifted,

    /// The provider holds an unsupported hand edit, which reconciliation
    /// reports and leaves in place until an operator removes it or
    /// re-asserts the projection.
    HandEdited,
}

impl ReconciliationStatus {
    /// A stable name, for log fields and API responses.
    ///
    /// Kept beside the serde attribute rather than derived from it, because
    /// the two are read by different consumers and neither should be able to
    /// change without the other being looked at.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Applied => "applied",
            Self::Failed => "failed",
            Self::Drifted => "drifted",
            Self::HandEdited => "handEdited",
        }
    }
}

impl std::fmt::Display for ReconciliationStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
