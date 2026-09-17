//! Which of the two reasons a realm could not be given to a new client.

/// Why [`ControlPlaneError::RealmUnavailable`](crate::ControlPlaneError::RealmUnavailable)
/// refused a realm.
///
/// A named type rather than a `bool`, so the reason reaches the operator's
/// message by being formatted, not by a call site inventing wording for it a
/// second time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealmUnavailableReason {
    /// This deployment never lets a client take this realm — see
    /// `fabric_control_plane_api::startup::reserved_names::realms`.
    Reserved,
    /// Another client's stored document already declares this realm.
    Taken,
}

impl std::fmt::Display for RealmUnavailableReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reserved => write!(formatter, "it is reserved by this deployment"),
            Self::Taken => write!(formatter, "another client already uses it"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RealmUnavailableReason;

    #[test]
    fn each_reason_names_itself_without_a_shared_word() {
        assert_eq!(
            RealmUnavailableReason::Reserved.to_string(),
            "it is reserved by this deployment"
        );
        assert_eq!(
            RealmUnavailableReason::Taken.to_string(),
            "another client already uses it"
        );
    }
}
