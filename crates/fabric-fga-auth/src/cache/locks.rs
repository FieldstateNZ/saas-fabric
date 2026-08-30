//! One lock per issuer, so refreshes for it are serialised.
//!
//! Bookkeeping rather than policy, kept apart from the decision it protects:
//! what makes an unknown key a refusal is in `cache.rs`, and nothing here has
//! an opinion about it.

use std::sync::Arc;

use tokio::sync::Mutex;

use super::held::Entry;
use super::KeyCache;

impl KeyCache {
    /// The lock for one issuer, created on first use.
    pub(super) async fn entry_for(&self, issuer: &str) -> Arc<Mutex<Entry>> {
        let mut entries = self.entries.lock().await;

        Arc::clone(
            entries
                .entry(issuer.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(Entry::default()))),
        )
    }
}
