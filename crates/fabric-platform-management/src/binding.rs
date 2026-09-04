//! Which platform repository is live, and swapping it.

#[cfg(test)]
#[path = "binding/binding_tests.rs"]
mod binding_tests;

use std::sync::Arc;

use tokio::sync::{RwLock, RwLockReadGuard};

use self::live::Live;

mod bound;
mod delegate;
mod generation;
mod live;
mod swap;

/// The platform repository this control plane is currently connected to.
///
/// # Why this is late-bound at all
///
/// The repository and the credential are not configuration any more. An
/// operator installs a GitHub App and picks a repository, and the platform
/// stores what it learns doing so — so at startup there is nothing to build
/// from, and a control plane that refused to start without one could not be
/// used to connect one. The same device the client desired-state binding uses,
/// for the same reason.
///
/// # Unconnected is a state, not an error
///
/// Every operation answers [`NotConnected`](crate::DesiredStateError::NotConnected)
/// while nothing is bound. The console renders that as "not connected"; a
/// *connected* repository that cannot be read answers something else, and an
/// operator is told which.
///
/// That distinction has a third case, and it is the one most easily got wrong.
/// An operator who connected a repository last week, whose application key can
/// no longer be read, must not be told "not connected" — they would go and
/// connect it again instead of finding out why it stopped working. See
/// [`unusable`](Self::unusable).
///
/// # An unbind waits, and a decision knows what it was read from
///
/// Changing the binding used to look free. Every operation cloned the
/// repository out of the lock and released it before awaiting, so a sweep that
/// began against repository A and an operator who disconnected half a second
/// later produced a commit landing in A *after* the platform had been told to
/// stop targeting it. A revision could not catch that: it proves the manifest
/// did not move, and A's manifest had not moved.
///
/// So an unbind **drains**. Every operation holds the read guard across the
/// await it delegates to, and [`connect`](Self::connect),
/// [`disconnect`](Self::disconnect) and [`unusable`](Self::unusable) take the
/// write guard — so they complete only once everything that began against the
/// old repository has finished, and nothing starts against it afterwards.
///
/// The wait is bounded because [`DesiredState`](crate::DesiredState) requires every
/// implementation to bound its operations, and a deployment's budget is checked at
/// startup to be shorter than one request. Without that the drain would be
/// unbounded: an operator's disconnect would queue behind a stalling Git host and
/// be cut off by the request timeout, leaving them a `504` and a platform still
/// pointed at the repository they asked it to forget.
///
/// And a decision is **tagged** with the generation of the binding it was read
/// through, because draining says nothing about a decision read a minute ago
/// and written now. `binding/live.rs` keeps that counter beside the repository
/// under the same lock; `binding/generation.rs` puts it on the revision, and
/// says why a mismatch is a
/// [`Conflict`](crate::DesiredStateError::Conflict) rather than a refusal.
pub struct PlatformDesiredState {
    /// What is bound, and which binding it is.
    ///
    /// An async lock rather than [`std::sync::RwLock`]: the guard is held
    /// across an await, which is the whole of the drain, and a blocking guard
    /// held across an await point is how a runtime deadlocks.
    current: RwLock<Live>,
}

impl PlatformDesiredState {
    /// A binding with no repository behind it.
    #[must_use]
    pub fn unconnected() -> Arc<Self> {
        Arc::new(Self {
            current: RwLock::new(Live::unconnected()),
        })
    }

    /// Whether a repository is live.
    ///
    /// `false` for a connected integration that could not be built: nothing can
    /// be read through it. What separates the two is the *error* callers get,
    /// not this.
    ///
    /// Async only because the lock is. There is nothing to wait for that a
    /// blocking read would not also have waited for — this answers from state
    /// already in memory, and the `.await` is the accessor's, not an I/O call's.
    pub async fn is_connected(&self) -> bool {
        self.live().await.repository().is_ok()
    }

    /// The binding, held for as long as the caller holds the guard.
    async fn live(&self) -> RwLockReadGuard<'_, Live> {
        self.current.read().await
    }
}
