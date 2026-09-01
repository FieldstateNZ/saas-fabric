//! Reconciling every component of an environment, on a schedule somebody else
//! keeps.

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
mod sweep_tests;

use crate::{PlatformError, PlatformManagement, Version};

/// What one component did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Swept {
    /// Desired state moved.
    Advanced {
        /// What it was on.
        from: Version,

        /// What it is on now.
        to: Version,
    },

    /// Nothing to do, or nothing permitted. The status says which.
    Unchanged,

    /// This component could not be reconciled.
    ///
    /// Carried rather than returned, because one component failing is not a
    /// reason to stop looking after the others — and a sweep that aborted on
    /// the first failure would leave every component after it in the list
    /// permanently unreconciled, in an order nobody chose.
    Failed(PlatformError),
}

/// What a sweep did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sweep {
    /// One entry per component, in the order they were read.
    pub components: Vec<(String, Swept)>,
}

impl Sweep {
    /// Whether anything failed.
    #[must_use]
    pub fn had_failures(&self) -> bool {
        self.components
            .iter()
            .any(|(_, swept)| matches!(swept, Swept::Failed(_)))
    }
}

/// Whether a sweep ran at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepResult {
    /// It ran, and this is what it did.
    Ran(Sweep),

    /// Another sweep was still going, so this one did nothing.
    ///
    /// Skipped rather than queued. A sweep that overruns its interval means
    /// registries or Git are slow, and the answer to that is not to start a
    /// second one behind it.
    AlreadyRunning,
}

/// Guards against two sweeps at once in one process.
#[derive(Debug, Default)]
pub struct SweepGuard {
    /// Whether a sweep is in progress.
    running: AtomicBool,
}

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
    /// # Errors
    ///
    /// [`PlatformError`] only if the environment's component list cannot be
    /// read. A component that fails is recorded and the sweep continues.
    pub async fn sweep(&self, environment: &str, guard: &SweepGuard) -> Result<SweepResult, PlatformError> {
        if guard.running.swap(true, Ordering::SeqCst) {
            return Ok(SweepResult::AlreadyRunning);
        }

        let swept = self.sweep_once(environment).await;
        guard.running.store(false, Ordering::SeqCst);

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
