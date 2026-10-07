//! One reload of one registry, shared by every refresh loop.

use crate::logging;
use crate::resource::{RegistryResource, ResourceRegistry, ResourceSource};

/// Reloads one registry from its source, once.
pub(super) async fn refresh_once<T: RegistryResource>(
    registry: &ResourceRegistry<T>,
    source: &dyn ResourceSource<T>,
) {
    match source.load().await {
        Ok(resources) => {
            if let Err(refused) = registry.apply_all(resources) {
                // Only reachable while the registry has never loaded — a prime
                // that was refused, and a source still publishing the payload
                // that got it refused. The registry stayed unprimed, which is
                // the whole point; this says so out loud.
                logging::first_load_refused::<T>(&source.describe(), &refused);
            }
        }
        Err(error) => {
            // Deliberately does not touch the registry. The last good snapshot
            // keeps serving; a momentarily unreadable source must not
            // deprovision everything.
            logging::refresh_failed::<T>(&source.describe(), &error);
        }
    }
}
