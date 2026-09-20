//! Reconciling every component of an environment, on a schedule somebody else
//! keeps.

#[cfg(test)]
mod sweep_tests;

mod record;
mod types;

pub use record::{CheckOutcome, LastCheck, SweepState};
pub use types::{Sweep, SweepResult, Swept};

use record::outcome_of;

use crate::{DesiredStateError, PlatformError, PlatformManagement};

impl PlatformManagement {
    /// Reconciles every component of an environment.
    ///
    /// # What this is not
    ///
    /// It is not a scheduler. Something else decides when to call it, which is
    /// what keeps the cadence configurable and this testable without a clock.
    ///
    /// It is also not a lock across replicas. Two control planes sweeping at
    /// once will both decide to advance, and the second one's write will be
    /// refused as stale — because the write's precondition is the desired
    /// state it was decided against. Duplicate sweeps are wasteful and not
    /// dangerous, which is why there is no leader election here.
    ///
    /// Within one process, re-entry *is* guarded: `state.running.try_enter()`
    /// hands back a guard only to the call that wins, released by `Drop`, so
    /// cancellation and a panic release it too, not only a normal return --
    /// see [`RuntimePublisher::publish_once`](crate::RuntimePublisher::publish_once)
    /// for the argument, which is the same guard.
    ///
    /// # Errors
    ///
    /// [`PlatformError`] only if the environment's component list cannot be
    /// read. A component that fails is recorded and the sweep continues.
    pub async fn sweep(&self, environment: &str, state: &SweepState) -> Result<SweepResult, PlatformError> {
        let Some(_guard) = state.running.try_enter() else {
            // Deliberately does not touch the record: a skipped sweep found
            // nothing because it did not look, and overwriting the last real
            // answer with that would hide it.
            return Ok(SweepResult::AlreadyRunning);
        };

        let swept = self.sweep_once(environment).await;

        // Nothing was looked at, so nothing is recorded. The record answers
        // "what did the last attempt find", and this was not an attempt.
        if matches!(
            swept,
            Err(PlatformError::DesiredState(DesiredStateError::NotConnected))
        ) {
            return Ok(SweepResult::NotConnected);
        }

        // Recorded while the guard is still held. Releasing it first --
        // as before -- let a second sweep start, finish, and record
        // before this one wrote its own older result, which then landed
        // last with a later timestamp than the newer record it erased.
        state.record(self.clock().now_unix_seconds(), outcome_of(&swept));

        swept.map(SweepResult::Ran)
    }

    /// The sweep itself, without the guard.
    async fn sweep_once(&self, environment: &str) -> Result<Sweep, PlatformError> {
        let components = self.desired_state().components(environment).await?;
        let mut sweep = Sweep::default();

        for component in components {
            let swept = match self.reconcile(environment, &component).await {
                Ok(reconciled) if reconciled.advanced() => Swept::Advanced {
                    from: reconciled.was,
                    to: reconciled.status.desired,
                },
                Ok(_) => Swept::Unchanged,
                Err(error) => {
                    // Recorded and logged, never swallowed. A sweep that hid
                    // this would look identical to one where nothing needed
                    // doing.
                    tracing::warn!(
                        environment,
                        component,
                        error = %error,
                        "a component could not be reconciled"
                    );
                    Swept::Failed(error)
                }
            };

            sweep.components.push((component, swept));
        }

        Ok(sweep)
    }
}
