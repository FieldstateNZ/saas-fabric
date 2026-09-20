//! Releasing [`PublicationState::running`](super::state::PublicationState)
//! on every path out of a pass, including the two a plain `store` after an
//! `.await` cannot reach.

use std::sync::atomic::Ordering;

use super::state::PublicationState;

/// Clears the running flag when dropped -- on a normal return, an early
/// return, cancellation (the caller drops the future holding this guard at
/// an await point), or a panic unwinding through it.
///
/// [`RuntimePublisher::publish_once`](super::publisher::RuntimePublisher::publish_once)
/// constructs this immediately after winning the atomic swap that sets the
/// flag, so nothing between that swap and this guard's own construction can
/// leave it set either.
pub(super) struct RunningGuard<'a> {
    pub(super) state: &'a PublicationState,
}

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.state.running.store(false, Ordering::SeqCst);
    }
}
