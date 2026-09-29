//! What is actually serving, as far as independent evidence shows.

/// A running version established by independent deployment evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Running {
    /// No single healthy running version is currently established.
    Unknown,
    /// All active observed workloads are healthy and agree on this version.
    Observed(String),
}
