//! Which connection, within a connector, a request should use.

use crate::{ConnectionName, SecretRef};

/// Selects the physical connection a request executes over.
///
/// A connector process serves many tenants. This is how one request says which
/// of its connections to use, and it is the mechanism that makes per-tenant
/// placement (§17) work without a connector per tenant.
///
/// # Prefer `Named`
///
/// The three variants are not equal in security terms:
///
/// - [`Self::Named`] keeps the credential inside the connector's own
///   configuration. Nothing sensitive travels on the request. **Use this
///   wherever possible.**
/// - [`Self::Secret`] sends a credential to the connector with each request. It
///   is necessary when a tenant has a dedicated database whose existence was
///   not known when the connector was configured, but it puts a secret in a
///   request body — so the value must never be logged, and the transport must
///   be authenticated.
/// - [`Self::Default`] uses the connector's single configured connection. Only
///   valid where one connector serves exactly one physical database.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectionSelector {
    /// Use the connector's single default connection.
    Default,

    /// Use a connection the connector already holds configuration for.
    Named {
        /// The connection's name in the connector's configuration.
        name: ConnectionName,
    },

    /// Use a connection built from a credential resolved at execution time.
    Secret {
        /// Where to find the credential (§21).
        reference: SecretRef,
    },
}

impl ConnectionSelector {
    /// A short, non-sensitive description for telemetry.
    ///
    /// For [`Self::Secret`] this reports the *reference*, never the resolved
    /// value — the reference is a path, and paths are safe to log (§29).
    #[must_use]
    pub fn telemetry_label(&self) -> String {
        match self {
            Self::Default => "default".to_owned(),
            Self::Named { name } => format!("named:{name}"),
            Self::Secret { reference } => format!("secret:{reference}"),
        }
    }

    /// Whether resolving this selector requires a secret lookup.
    #[must_use]
    pub const fn needs_secret(&self) -> bool {
        matches!(self, Self::Secret { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_telemetry_label_for_a_secret_reports_the_path_not_a_value() {
        let selector = ConnectionSelector::Secret {
            reference: SecretRef::new("tenant/acme/data-primary"),
        };

        assert_eq!(selector.telemetry_label(), "secret:tenant/acme/data-primary");
    }

    #[test]
    fn only_the_secret_variant_needs_a_lookup() {
        assert!(!ConnectionSelector::Default.needs_secret());
        assert!(ConnectionSelector::Secret {
            reference: SecretRef::new("x")
        }
        .needs_secret());
    }
}
