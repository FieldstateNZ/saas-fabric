//! Why a registry lookup did not produce a resource.

/// The two ways a lookup can fail, kept apart on purpose.
///
/// Callers map this onto their own domain error. The distinction must survive
/// that mapping: [`Self::Unavailable`] is a platform problem and retryable,
/// [`Self::NotFound`] is a statement about the resource. Conflating them during
/// a cold start would tell every caller their tenant had been deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupError {
    /// The registry has never loaded a snapshot, so nothing can be resolved.
    Unavailable,

    /// A snapshot is loaded and does not contain the requested key.
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_failures_are_distinguishable() {
        // If these ever compare equal, a cold start becomes indistinguishable
        // from a deleted resource.
        assert_ne!(LookupError::Unavailable, LookupError::NotFound);
    }
}
