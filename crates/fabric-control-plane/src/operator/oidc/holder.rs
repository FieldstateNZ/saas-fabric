//! Holding the current key set while requests are being served.

use std::sync::{Arc, RwLock};

use super::keys::VerificationKeys;

/// The current key set, swappable while requests are being served.
///
/// Starts empty. A control plane whose provider is unreachable at startup must
/// still start and serve — refusing operator requests until a key set arrives
/// is a degraded authentication surface, not a dead process.
#[derive(Default)]
pub struct KeyHolder {
    /// The keys, behind a lock held only long enough to clone the `Arc`.
    current: RwLock<Arc<VerificationKeys>>,
}

impl KeyHolder {
    /// A holder with no keys yet.
    #[must_use]
    pub fn empty() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Replaces the key set.
    pub fn replace(&self, keys: VerificationKeys) {
        match self.current.write() {
            Ok(mut held) => *held = Arc::new(keys),

            // A poisoned lock means a thread panicked while holding it. This
            // one only ever swaps an `Arc`, so there is no torn state to
            // protect and refusing every operator afterwards would be the
            // worse outcome.
            Err(poisoned) => *poisoned.into_inner() = Arc::new(keys),
        }
    }

    /// The current key set.
    pub(super) fn current(&self) -> Arc<VerificationKeys> {
        match self.current.read() {
            Ok(held) => Arc::clone(&held),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }
}
