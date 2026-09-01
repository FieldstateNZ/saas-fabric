//! The three things a platform binding can be holding.

use std::sync::Arc;

use crate::{DesiredState, SafeDiagnostic};

/// What the binding currently holds.
///
/// Three states rather than an `Option`, because "nobody has connected one"
/// and "somebody connected one and this platform cannot use it" are different
/// facts about a deployment and lead an operator to different actions. An
/// `Option` can only say the first, which is why the second used to look like
/// it.
pub(super) enum Bound {
    /// Nobody has connected a repository.
    Nothing,

    /// A repository, live.
    Repository(Arc<dyn DesiredState>),

    /// An operator connected one, and this platform could not build or
    /// authenticate against it.
    ///
    /// Reported as unavailable, which is what it is: the integration exists,
    /// and reading through it does not work.
    Unusable(SafeDiagnostic),
}
