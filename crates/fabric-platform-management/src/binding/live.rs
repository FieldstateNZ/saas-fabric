//! What the binding holds, and which binding it is.

use std::sync::Arc;

use crate::binding::bound::Bound;
use crate::{DesiredState, DesiredStateError};

/// The guarded half of a platform binding.
///
/// # Why the generation lives here and not beside the lock
///
/// The counter and the repository have to move together or the counter is
/// worthless: a decision tagged with generation 4 is only meaningful if
/// generation 4 named exactly one repository, for the whole time it was
/// current. Two locks, or a counter outside the lock, would let a reader see
/// generation 4 with generation 5's repository — which is precisely the
/// mismatch the tag exists to catch, reintroduced one level down.
///
/// So they are one value under one lock, and the only way to change either is
/// [`replace`](Self::replace), which changes both.
pub(super) struct Live {
    /// How many times a repository has been bound, unbound or declared
    /// unusable.
    ///
    /// Starts at zero, which is the generation of a platform nobody has
    /// connected. Nothing reads it as a count — it exists only to be unequal
    /// to its predecessor.
    generation: u64,

    /// What is bound now.
    bound: Bound,
}

impl Live {
    /// A binding with no repository behind it.
    pub(super) const fn unconnected() -> Self {
        Self {
            generation: 0,
            bound: Bound::Nothing,
        }
    }

    /// Replaces what is bound, and moves on from the generation that held it.
    ///
    /// Wrapping rather than saturating, because a saturated counter would stop
    /// distinguishing generations and start silently accepting stale
    /// decisions. Reaching `u64::MAX` would take longer than the platform will
    /// exist; a wrap that did happen is still a change of value, which is all
    /// the tag compares.
    pub(super) fn replace(&mut self, bound: Bound) {
        self.generation = self.generation.wrapping_add(1);
        self.bound = bound;
    }

    /// Which binding this is.
    pub(super) const fn generation(&self) -> u64 {
        self.generation
    }

    /// The live repository, or the refusal that says why there is none.
    ///
    /// Hands back the [`Arc`] rather than a `&dyn DesiredState`, because the
    /// task that runs the operation outlives this borrow: a reference into the
    /// guard could not be moved into it. Cloning an `Arc` to answer
    /// [`is_connected`](super::PlatformDesiredState::is_connected) is a cheap
    /// price for that.
    pub(super) fn repository(&self) -> Result<Arc<dyn DesiredState>, DesiredStateError> {
        match &self.bound {
            Bound::Nothing => Err(DesiredStateError::NotConnected),
            Bound::Repository(repository) => Ok(Arc::clone(repository)),
            Bound::Unusable(detail) => Err(DesiredStateError::Unavailable {
                detail: detail.as_str().to_owned(),
            }),
        }
    }
}
