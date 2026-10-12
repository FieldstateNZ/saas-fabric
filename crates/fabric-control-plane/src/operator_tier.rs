//! The realm tier `GET /api/user/current` reports.

use std::sync::Arc;

/// The one role every operator the deployment's authenticator accepts holds,
/// as `GET /api/user/current` presents it (ADR 0024 §3).
///
/// # A label, not an authority
///
/// Fixed when the router is built, from the posture that decides who is an
/// operator, and never read from a token: a token's own realm roles include
/// whatever else the realm grants, and the route reports the tier, not a
/// permission list. Nothing on the server reads this to allow anything, and
/// a browser must not either — every route still takes an
/// [`Operator`](crate::Operator), which is the only proof of authority here.
#[derive(Debug, Clone)]
pub(crate) struct OperatorTier(Arc<str>);

impl OperatorTier {
    /// The tier for an authenticator that requires `role`.
    ///
    /// # Errors
    ///
    /// Returns a message if `role` is blank, which would report a tier no
    /// operator holds.
    pub(crate) fn new(role: &str) -> Result<Self, String> {
        let role = role.trim();
        if role.is_empty() {
            return Err("operator: the presented role must not be empty".to_owned());
        }
        Ok(Self(Arc::from(role)))
    }

    /// The role, as the route reports it.
    pub(crate) fn role(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::OperatorTier;

    #[test]
    fn reports_the_role_trimmed() {
        assert_eq!(
            OperatorTier::new("  platform-admin ").unwrap().role(),
            "platform-admin"
        );
    }

    #[test]
    fn a_blank_role_is_refused_rather_than_reported() {
        assert!(OperatorTier::new("").is_err());
        assert!(OperatorTier::new("   ").is_err());
    }
}
