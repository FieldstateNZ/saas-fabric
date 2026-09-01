//! What can go wrong reading or moving desired state.

/// What can go wrong reading or moving desired state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DesiredStateError {
    /// Nothing is connected. No operator has connected a platform repository.
    ///
    /// # Not a failure, and not the same as one
    ///
    /// A platform nobody has connected yet is a running platform waiting for
    /// an operator, and a console can say so. A platform whose *connected*
    /// repository cannot be read is broken and needs looking at.
    ///
    /// Collapsing them would tell an operator "nothing is connected" about an
    /// integration they connected last week, and they would go and connect it
    /// again rather than find out why it stopped working.
    #[error("no platform repository is connected")]
    NotConnected,

    /// No such environment, or no such component in it.
    #[error("{what} is not something this platform describes")]
    NotFound {
        /// What was asked for.
        what: String,
    },

    /// Something the write was editing changed since it was read.
    ///
    /// Not a failure so much as an instruction: the decision was taken against
    /// state that has moved, so it has to be taken again.
    #[error("desired state changed while it was being written")]
    Conflict,

    /// The store could not be reached, or failed internally.
    #[error("desired state is unavailable: {detail}")]
    Unavailable {
        /// What was observed, with no credential in it.
        detail: String,
    },

    /// The store understood the request and refused it.
    #[error("desired state refused the change: {detail}")]
    Refused {
        /// What was observed, with no credential in it.
        detail: String,
    },
}
