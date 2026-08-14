//! Who may be told *why* this replica is unready.
//!
//! # The problem
//!
//! `/ready` is merged into the same router as `/v1/data`, on the port
//! applications are meant to reach. It used to answer every caller with
//! connector ids (`postgres-au-east` — physical identity, with a region hint),
//! the size of the estate, and a raw `ConnectorError::to_string()` whose
//! `Rejected` variant's own docs say it must not be passed to an application
//! verbatim. All of it with no credentials at all, beside a `/v1/data` that
//! correctly answers 401.
//!
//! The trusted-ingress posture does not cover this. That posture says to fix
//! the network boundary rather than harden one hop — but this *is* the
//! boundary applications are meant to be on the far side of, so it draws no
//! line between the two surfaces at all.
//!
//! # Why this shape, and not the others
//!
//! - **A separate admin listener** is the most orthodox answer and was the
//!   closest call. It was not taken because it moves the problem rather than
//!   solving it: a second in-cluster port is still reachable by every pod
//!   unless a `NetworkPolicy` says otherwise, so the detail would still be
//!   unauthenticated — just on a port with fewer eyes on it — while costing a
//!   new config setting, a second bind, and a probe repointing in every
//!   deployment manifest.
//! - **A fixed generic reason** keeps the estate secret but destroys the
//!   diagnosis path §34 asks for: an operator would be left with "something is
//!   unhealthy" and no way to learn what.
//! - **This**: the verdict is public, the detail is authorised. The verdict is
//!   a single bit an orchestrator already infers from the status code, so
//!   publishing it discloses nothing the port did not already disclose.
//!
//! # How a kubelet copes
//!
//! It does not have to. `readinessProbe.httpGet` reads the **status code** and
//! never parses the body, so an unauthenticated probe still gets its whole
//! answer: 200 or 503, unchanged, identical to what an authorised caller gets.
//! Nothing about the probe contract depends on the body — which is exactly why
//! the body is the safe place to put the part that needs a credential.
//!
//! An operator diagnosing a replica presents the same platform-admin token the
//! Data API already recognises. No new credential, no new configuration.

use http::HeaderMap;

use crate::health::HealthState;

/// Whether this caller may be shown the estate detail.
///
/// Deliberately fails closed and silently: a caller who cannot see the detail
/// gets the minimal body and the same status code, never a 401. A probe
/// endpoint that rejects unauthenticated callers outright would be unusable by
/// the orchestrator it exists for.
pub(super) fn may_see_detail(state: &HealthState, headers: &HeaderMap) -> bool {
    if state.administrator_role.is_empty() {
        // An empty role name would otherwise match a token carrying an empty
        // role string. A deployment that configures no administrator has
        // authorised nobody, which is the fail-closed reading.
        //
        // Kept as defence in depth even though `config::administrator_role`
        // now refuses a blank value at startup: `HealthState` is constructible
        // without going through `AppConfig`, so this guard covers the paths
        // that validation never sees.
        return false;
    }

    state
        .identity
        .resolve(headers)
        .is_ok_and(|identity| identity.has_role(&state.administrator_role))
}
