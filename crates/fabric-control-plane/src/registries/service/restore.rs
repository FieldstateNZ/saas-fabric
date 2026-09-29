//! Picking up what operators registered before the last restart.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use crate::registries::logging::restore_incomplete;
use crate::registries::service::RegistryService;

impl RegistryService {
    /// Builds a client for every recorded registry and installs the set, at
    /// startup.
    ///
    /// # Never fatal
    ///
    /// A control plane that refused to start because a registry record or
    /// credential could not be read could not be used to fix either. So each
    /// failure is logged and the rest carry on: a record set that cannot be
    /// read leaves only the deployment's anonymous registry; a credential that
    /// cannot be read leaves its registry read anonymously, and listed as
    /// such; a registry for the deployment's host whose endpoint is no longer
    /// the deployment's is not read through at all, because its credential
    /// was never for that endpoint. Each credential is presented once more,
    /// as after any restart.
    ///
    /// Answers whether everything that could be read was: `false` when the
    /// record set or a credential could not be read *now* — a store that may
    /// answer shortly, which [`restore_until_complete`](Self::restore_until_complete)
    /// asks again. A credential the store answered is not there is not
    /// retried: no wait brings it back, and the operator sets it again.
    pub async fn restore(&self) -> bool {
        let inner = &self.inner;
        let _turn = inner.order.lock().await;
        let records = match inner.store.load().await {
            Ok(records) => records,
            Err(error) => {
                restore_incomplete(None, &error.to_string());
                inner.swap_all(BTreeMap::new());
                return false;
            }
        };

        let mut complete = true;
        let mut live = BTreeMap::new();
        for record in records {
            let (entry, read) = inner.restored(&record).await;
            complete &= read;
            if let Some(entry) = entry {
                live.insert(record.host, entry);
            }
        }
        inner.swap_all(live);
        complete
    }

    /// After a [`restore`](Self::restore) that answered `false`: restores
    /// again, after a wait that doubles from five seconds to five minutes,
    /// until everything that could be read was.
    ///
    /// # Why it asks again at all
    ///
    /// A secret store that did not answer while the control plane started —
    /// an `OpenBao` restarting beside it — would otherwise leave every
    /// registry unrestored, or read anonymously, until the next restart or
    /// until a person typed each token again. Each attempt takes its turn
    /// like any change and reads the store afresh, so it never undoes one an
    /// operator made in the meantime, and keeps every credential's refusal.
    pub async fn restore_until_complete(self: Arc<Self>) {
        let mut wait = Duration::from_secs(5);
        loop {
            tokio::time::sleep(wait).await;
            if self.restore().await {
                return;
            }
            wait = (wait * 2).min(Duration::from_secs(300));
        }
    }
}
