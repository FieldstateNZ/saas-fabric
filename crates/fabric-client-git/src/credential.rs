//! The platform's machine credential for the desired-state repository.

/// The token SaaS Fabric authenticates to the Git host with.
///
/// Redacting newtype, for the same reason
/// `AdminCredential` is one in the Keycloak adapter: a
/// `String` here is one `{:?}` away from putting a token with write access to
/// every client's desired state into a log aggregator, and the code that leaks
/// it looks exactly like the code that does not.
///
/// # What must supply it
///
/// A machine identity with write access to the desired-state repository and
/// nothing else — a GitHub App installation token or a fine-grained token
/// scoped to that one repository. Never a human's personal access token, and
/// never anything the operator UI could supply.
#[derive(Clone)]
pub struct GitCredential(String);

impl GitCredential {
    /// Wraps a resolved token value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the token, for the one place that must send it.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for GitCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("GitCredential(redacted)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_does_not_contain_the_token() {
        let credential = GitCredential::new("ghp_notarealtoken");

        assert!(!format!("{credential:?}").contains("ghp_"));
    }
}
