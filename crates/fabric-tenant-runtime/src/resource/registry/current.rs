//! Handing the request path the snapshot currently installed.

use std::sync::Arc;

use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{RegistryResource, ResourceRegistry};

impl<T: RegistryResource> ResourceRegistry<T> {
    /// The snapshot currently installed, or `None` while unprimed.
    ///
    /// Resolution reads every fact it needs -- the resource and the
    /// snapshot's whole-set facts -- off one returned snapshot, rather than
    /// loading the registry once per fact, so a swap mid-request cannot
    /// answer one question from the old set and the next from the new one.
    /// An atomic load and an `Arc` clone, the same budget as
    /// [`Self::lookup`].
    pub(crate) fn current(&self) -> Option<Arc<ResourceSnapshot<T>>> {
        self.snapshot.load_full()
    }
}
