//! Whether a registry would take a set exactly as offered, judged before
//! anything is written.

use std::collections::BTreeSet;

use crate::resource::{RegistryResource, ResourceRegistry};

impl<T: RegistryResource> ResourceRegistry<T> {
    /// Why [`Self::apply_all`] would not take every resource of `incoming`
    /// as offered, or `None` when it would.
    ///
    /// The same rules the merge applies, asked without writing: an invalid
    /// resource, a key twice, a revision below the one held, or a changed
    /// payload at the revision held would each leave the registry holding
    /// something other than the set offered. A recorded generation is
    /// installed whole or not at all (G8), so the refresher asks both
    /// registries this before writing either. Only the refresher writes a
    /// published runtime's registries, so the answer still holds when it
    /// applies.
    pub(crate) fn refuses_whole(&self, incoming: &[T]) -> Option<String> {
        let guard = self.snapshot.load();
        let mut seen = BTreeSet::new();
        for resource in incoming {
            let key = resource.key();
            if let Err(error) = resource.validate() {
                return Some(format!("{} {key}: {error}", T::KIND));
            }
            if !seen.insert(key) {
                return Some(format!("{} {key} appears twice", T::KIND));
            }
            let Some(held) = guard.as_deref().and_then(|snapshot| snapshot.get(key)) else {
                continue;
            };
            if resource.revision() < held.revision() {
                return Some(format!(
                    "{} {key} is offered at revision {}, below revision {} already applied",
                    T::KIND,
                    resource.revision(),
                    held.revision()
                ));
            }
            if resource.revision() == held.revision() && resource != held.as_ref() {
                return Some(format!(
                    "{} {key} changed without its revision {} advancing",
                    T::KIND,
                    resource.revision()
                ));
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "takes_whole_tests.rs"]
mod tests;
