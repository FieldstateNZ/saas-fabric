//! The seam between a reference and the credential it points at.

use async_trait::async_trait;

use super::{ResolvedSecret, SecretRef};
use crate::ConnectorError;

/// Resolves a [`SecretRef`] to its value.
///
/// The implementation is a deployment concern. Applications never receive or
/// understand the physical secret location (§21), and neither does anything in
/// this crate above this trait.
///
/// Implementations are expected to cache. This sits on the request path when a
/// tenant uses [`ConnectionSelector::Secret`](crate::ConnectionSelector), and a
/// round trip to a secret store per data operation would be a poor trade.
///
/// # Contract: do not be coarser than [`SecretRef::distinctness_key`]
///
/// **Two references whose distinctness keys differ must not resolve to one
/// credential.** An implementation that cannot honour that — because the
/// namespace it projects into is narrower still — must fail the resolution
/// rather than serve the collision.
///
/// This is a real obligation rather than a formality, and it is stated here
/// rather than left to each implementation because of where the consequence
/// lands. `fabric-tenant-runtime` refuses structural isolation when two
/// DataSources reach one destination, and it decides "one destination" for
/// secret-backed connections by comparing distinctness keys — no I/O, because
/// §6 keeps the request path out of the control plane. A resolver that
/// collapses two references the key keeps apart therefore puts two tenants on
/// one database with no predicate and no error anywhere in the stack.
///
/// The key's own documentation sets out what it discards, which is exactly what
/// an implementation is free to discard too: letter case, and the identity of
/// every non-alphanumeric character.
///
/// The converse is not required and cannot be. Two references with *different*
/// keys pointing at one credential — Vault aliases, two store entries holding
/// one connection string — is a deployment fact no comparison of strings can
/// see, and it remains outside what any layer above this trait detects.
#[async_trait]
pub trait SecretResolver: Send + Sync {
    /// Resolves a secret reference.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::SecretUnavailable`] when the reference cannot
    /// be resolved. This fails the request closed — there is no fallback
    /// credential (§28).
    async fn resolve(&self, reference: &SecretRef) -> Result<ResolvedSecret, ConnectorError>;
}
