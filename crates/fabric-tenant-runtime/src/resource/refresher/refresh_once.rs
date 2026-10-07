//! One reload of one registry, shared by every refresh loop.

use std::time::Duration;

use crate::logging;
use crate::resource::{RegistryResource, ResourceRegistry, ResourceSource};

/// Reloads one registry from its source, once, giving up after `timeout`.
///
/// The timeout exists because one loop can reload several registries in
/// turn: without it, a source that never answers would stop every registry
/// after it from refreshing, deprovisioning included. A reload that times out
/// leaves the registry untouched, like one that fails.
pub(super) async fn refresh_once<T: RegistryResource>(
    registry: &ResourceRegistry<T>,
    source: &dyn ResourceSource<T>,
    timeout: Duration,
) {
    let Ok(loaded) = tokio::time::timeout(timeout, source.load()).await else {
        logging::refresh_timed_out::<T>(&source.describe(), timeout.as_secs());
        return;
    };

    match loaded {
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
