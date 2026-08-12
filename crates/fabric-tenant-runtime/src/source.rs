//! Where reconciled bindings come from.

use async_trait::async_trait;

use crate::{BindingSourceError, TenantRuntimeBinding};

/// Supplies the reconciled runtime bindings.
///
/// # What may implement this, and what may not
///
/// An implementation reads state that **reconciliation has already produced**.
/// A mounted file that an operator writes, a runtime store the controller
/// updates, an internal API in front of one — all fine.
///
/// What must not implement this is anything that reaches back into the control
/// plane: a Git client, a Kubernetes API client walking custom resources, a
/// cloud provider's describe API. §6 prohibits Git in request handling, and
/// while this trait is not itself on the request path, an implementation that
/// queries the control plane puts control-plane availability directly behind
/// data-plane availability. When Git is down, tenants should keep working.
///
/// # Failure is not emptiness
///
/// Returning `Err` leaves the registry's current snapshot untouched. Returning
/// `Ok(vec![])` **removes every tenant**. An implementation that cannot read
/// its source must return `Err`; turning a read failure into an empty set would
/// take the whole platform down (§28 — the runtime fails closed, but it does
/// not invent a closed state out of an I/O error).
#[async_trait]
pub trait BindingSource: Send + Sync {
    /// Loads the complete current set of tenant bindings.
    ///
    /// # Errors
    ///
    /// [`BindingSourceError`] if the source cannot be read or understood.
    /// Never return `Ok` with a partial set — a partial set is a removal.
    async fn load(&self) -> Result<Vec<TenantRuntimeBinding>, BindingSourceError>;

    /// A short description for logging, such as a file path or endpoint.
    fn describe(&self) -> String;
}
