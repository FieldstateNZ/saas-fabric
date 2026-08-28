//! The platform's machine credential for Keycloak.

/// The client secret SaaS Fabric authenticates to Keycloak with.
///
/// # A newtype whose entire purpose is what it will not do
///
/// It has no [`Display`](std::fmt::Display), and its [`Debug`] prints a fixed
/// string. A `String` in the same place would be one `{:?}` in an error type,
/// one `#[derive(Debug)]` on a config struct, or one `tracing::error!` away
/// from putting the platform's Keycloak administrative secret in a log
/// aggregator — and nothing about that failure is visible in review, because
/// the code that leaks it looks exactly like the code that does not.
///
/// This is the same device `ResolvedSecret` uses in the runtime plane. It is
/// deliberately *not* that type: sharing it would put a runtime-plane crate in
/// the control plane's dependency graph for the sake of forty lines, and the
/// two planes' graphs are kept apart on purpose.
///
/// # What must supply it
///
/// A machine identity — a confidential Keycloak client created for SaaS
/// Fabric, with only the realm-management permissions reconciliation needs.
/// Never a human administrator's password, never a browser session, and never
/// anything the operator UI could supply (§20).
#[derive(Clone)]
pub struct AdminCredential(String);

impl AdminCredential {
    /// Wraps a resolved secret value.
    ///
    /// The value is expected to arrive from the platform's secret delivery —
    /// see [`crate::KeycloakConfig::client_secret_ref`] for the contract this
    /// application defines and the deployment satisfies.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the secret, for the one place that must send it.
    ///
    /// `pub(crate)` and named to be conspicuous: a reviewer seeing
    /// `expose()` outside the token exchange should ask why.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for AdminCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AdminCredential(redacted)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_does_not_contain_the_secret() {
        let credential = AdminCredential::new("s3cr3t-value");

        assert!(!format!("{credential:?}").contains("s3cr3t"));
    }

    #[test]
    fn a_struct_deriving_debug_around_it_stays_safe() {
        // The realistic leak: not `{:?}` on the credential, but `{:?}` on a
        // config struct that happens to hold one.
        #[derive(Debug)]
        struct Holder {
            #[allow(dead_code)]
            credential: AdminCredential,
        }

        let holder = Holder {
            credential: AdminCredential::new("s3cr3t-value"),
        };

        assert!(!format!("{holder:?}").contains("s3cr3t"));
    }
}
