//! Correlating a Git-host round trip with the operator who started it.
//!
//! # Why the callbacks cannot be authenticated the ordinary way
//!
//! The Git host redirects the operator's *browser* back to this API. That
//! redirect carries no bearer token — the console holds the token in memory
//! and cannot attach it to a navigation it does not make — so the callback
//! handlers cannot take an `Operator` extractor. Something else has to prove
//! the callback belongs to a flow this platform started.
//!
//! # Server-side and single-use, rather than signed
//!
//! Workspec signs a stateless blob and verifies it within a ten-minute window.
//! That is the right answer when callbacks may land on any of several
//! processes, and it has a cost it does not hide: nothing is *consumed*, so a
//! captured state is replayable for as long as it is valid.
//!
//! This platform runs one control-plane process, so it can do better. A flow
//! is a random token held here and removed when it is used, which makes replay
//! impossible rather than merely brief, and needs no signing key.
//!
//! The trade is stated rather than discovered: an in-flight flow does not
//! survive a restart, and would not survive a second replica. Both mean "start
//! the connection again", the flow takes seconds, and neither can produce a
//! wrong outcome — only a repeated one.

use std::collections::HashMap;
use std::sync::Mutex;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;

/// Which leg of the connection a token was issued for.
///
/// Checked on use, so a token issued to create an application cannot be
/// presented to the installation callback. The two callbacks do different
/// things with different inputs, and a token that worked at either would let a
/// half-finished flow be steered into the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowStep {
    /// Creating the application on the host.
    Creation,

    /// Installing it on an account.
    Installation,
}

/// A flow that has been started and not yet completed.
#[derive(Debug, Clone)]
pub struct PendingFlow {
    /// The operator who started it, for the audit record.
    pub operator: String,

    /// Which leg it is.
    pub step: FlowStep,

    /// When it stops being usable, in Unix seconds.
    pub expires_at: u64,
}

/// How long a started flow remains usable.
///
/// Ten minutes: long enough for an operator to read a GitHub approval screen
/// and think about it, short enough that an abandoned flow is not left open
/// for an afternoon.
pub const FLOW_LIFETIME_SECONDS: u64 = 10 * 60;

/// The flows this process has started and not yet seen completed.
#[derive(Default)]
pub struct PendingFlows {
    /// Token to flow.
    entries: Mutex<HashMap<String, PendingFlow>>,
}

impl PendingFlows {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a flow, returning the token the host must return.
    ///
    /// # Errors
    ///
    /// Returns a message if the system's randomness is unavailable. Failing is
    /// the only option: a predictable token is one somebody else can present.
    pub fn begin(&self, operator: &str, step: FlowStep, now: u64) -> Result<String, String> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| "the system's randomness is unavailable".to_owned())?;

        let token = URL_SAFE_NO_PAD.encode(bytes);

        let mut entries = self.lock();
        entries.retain(|_, flow| flow.expires_at > now);
        entries.insert(
            token.clone(),
            PendingFlow {
                operator: operator.to_owned(),
                step,
                expires_at: now + FLOW_LIFETIME_SECONDS,
            },
        );

        Ok(token)
    }

    /// Spends a token, if it names a live flow of the expected step.
    ///
    /// **Removes it either way it matches.** A token is usable once; a second
    /// presentation of the same one is a replay and is refused.
    pub fn consume(&self, token: &str, step: FlowStep, now: u64) -> Option<PendingFlow> {
        let flow = self.lock().remove(token)?;

        (flow.step == step && flow.expires_at > now).then_some(flow)
    }

    /// How many flows are outstanding, for tests and for a log line.
    #[must_use]
    pub fn outstanding(&self) -> usize {
        self.lock().len()
    }

    /// The entries, treating a poisoned lock as usable.
    ///
    /// Only a map of short-lived tokens is behind it, so there is no torn
    /// state to protect and refusing every connection afterwards would be the
    /// worse outcome.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, PendingFlow>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
