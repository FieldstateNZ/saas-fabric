//! Every change takes its turn, runs to the end, and is audited.
//!
//! # Why one lock, held across the whole change
//!
//! A change writes up to three places — a credential, the record set, and
//! the live client Platform Management reads through — after proving
//! something over the network. Two changes interleaved could record one
//! credential while installing a client built with another, or save a record
//! set read before the other change saved its own. Held across the whole of
//! each, the platform ends on whichever ran last, whole.
//!
//! The lock is a `tokio` mutex in this process, and the record set is read
//! whole and written back whole beneath it. Both are stated rather than
//! hidden, because together they bound what the order proves: one process's
//! changes, against a store that holds the last whole set written. A desired
//! replica count of one does not extend that across an upgrade. A default
//! rolling update starts a pod's successor before the pod stops, so for that
//! interval two processes each take their own turn against the same store,
//! each can read the set and write back over the other's write, and each
//! holds its own refusal marks (ADR 0026 section 5). Before registry writes
//! are enabled across an upgrade, the rollout needs a reviewed way to prevent
//! that overlap — a single writer by drain and verified termination, or
//! coordination between writers — with the availability it costs approved.
//! Neither is chosen here, and neither a replica count nor a drain is on its
//! own fencing against a pod deleted by hand or a partitioned node. That is
//! an activation gate still open, not an architecture this module selected.
//!
//! # Why in a task of its own
//!
//! Proving a credential asks a registry and every repository registered
//! under it, and that can outlast a request's timeout. The change runs in a
//! task the request only awaits, so a request that is cut off does not stop
//! a change half-way between writing a credential and recording it; the
//! operator may see `504` and find the change made.

use std::future::Future;
use std::sync::Arc;

use crate::audit::{registry_changed, RegistryOperation};
use crate::registries::service::{Inner, RegistryService};
use crate::registries::{RegistryFailure, RegistryHost};
use crate::Operator;

impl RegistryService {
    /// Runs `work` in its turn, in a task of its own, and audits how it
    /// turned out.
    pub(super) async fn change<T, Work, Done>(
        &self,
        operator: &Operator,
        host: Option<RegistryHost>,
        operation: RegistryOperation,
        work: Work,
    ) -> Result<T, RegistryFailure>
    where
        Work: FnOnce(Arc<Inner>) -> Done,
        Done: Future<Output = Result<T, RegistryFailure>> + Send + 'static,
        T: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        let subject = operator.subject().to_owned();
        let done = work(Arc::clone(&inner));

        let task = tokio::spawn(async move {
            let result = {
                let _turn = inner.order.lock().await;
                done.await
            };
            registry_changed(&subject, host.as_ref(), operation, outcome(&result));
            result
        });

        task.await.unwrap_or_else(|_| {
            Err(RegistryFailure::Unavailable(
                "the registry change was not observed to finish".to_owned(),
            ))
        })
    }

    /// Audits a refusal made before a change took its turn: a request the
    /// rules refused outright.
    pub(super) fn refused<T>(
        operator: &Operator,
        host: Option<&RegistryHost>,
        operation: RegistryOperation,
        failure: RegistryFailure,
    ) -> Result<T, RegistryFailure> {
        registry_changed(operator.subject(), host, operation, failure.code());
        Err(failure)
    }
}

/// `succeeded`, or the refusal's stable code.
fn outcome<T>(result: &Result<T, RegistryFailure>) -> &'static str {
    match result {
        Ok(_) => "succeeded",
        Err(failure) => failure.code(),
    }
}
