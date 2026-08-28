//! Why a registry refused the first set it was ever offered.

/// A registry's **first** load published resources and none of them could be
/// served.
///
/// Applying a set primes the registry as a side effect, and nothing can
/// un-prime it — so the first load is the one apply that has to be able to *not
/// happen*. Installing an empty snapshot there flips `/ready` to 200 and joins
/// the replica to the load balancer, while every request turns from a retryable
/// [`ResolveError::RuntimeUnavailable`](crate::ResolveError) into a terminal
/// [`ResolveError::UnknownTenant`](crate::ResolveError) telling callers their
/// tenant does not exist. That transition is irreversible.
///
/// Only [`ResourceRegistry::apply_all`](crate::ResourceRegistry::apply_all)
/// produces this, and only on a registry that has never loaded. Once a snapshot
/// exists, a rejected resource falls back to the copy already held, so there is
/// always something left to serve and no later apply can fail this way.
#[derive(Debug, thiserror::Error)]
#[error("none of the {published} resources published could be served; first rejection: {reason}")]
pub struct UnusableFirstLoad {
    /// How many resources the set offered, every one of them unusable.
    ///
    /// Carried on the error rather than left to the caller to remember: the
    /// caller has already handed `incoming` over by the time this comes back.
    pub published: usize,

    /// The first rejection, named, so a refusal says what to go and fix rather
    /// than only that something is wrong.
    pub reason: String,
}
