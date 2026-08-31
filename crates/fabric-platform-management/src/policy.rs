//! How a component's version is allowed to move.

/// The standing decision about advancement.
///
/// Owned here rather than by whichever adapter happens to store it: this is
/// what the selector reads, and a policy is a rule before it is a field in a
/// file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdatePolicy {
    /// The newest eligible version is selected without asking.
    Automatic,

    /// An update is surfaced, and an operator chooses it.
    Manual,

    /// Nothing moves without a deliberate change to the constraint itself.
    Locked,
}
