//! Reading a snapshot's whole-set facts from the request path.

use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{RegistryResource, ResourceRegistry};

impl<T: RegistryResource> ResourceRegistry<T> {
    /// Reads the whole-set facts of the snapshot currently installed.
    ///
    /// `None` while the registry is unprimed — there is no set yet, so there
    /// is nothing true about it. Callers must not read that as "nothing is
    /// shared": an unprimed registry fails resolution earlier, at
    /// [`Self::lookup`], with [`LookupError::Unavailable`](crate::LookupError).
    ///
    /// # Why a closure rather than a return value
    ///
    /// The facts live inside the snapshot, behind `arc-swap`'s load guard.
    /// Handing them out would mean either cloning them on every request or
    /// wrapping them in a second `Arc` purely to escape the guard. Passing the
    /// borrow into a closure keeps the guard alive for exactly as long as the
    /// read, and costs an atomic load and a borrow — the same budget
    /// [`Self::lookup`] works to.
    pub(crate) fn with_set_facts<R>(&self, read: impl FnOnce(Option<&T::SetFacts>) -> R) -> R {
        let guard = self.snapshot.load();

        read(guard.as_deref().map(ResourceSnapshot::facts))
    }
}
