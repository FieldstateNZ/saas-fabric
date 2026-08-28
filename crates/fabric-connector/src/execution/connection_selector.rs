//! Which connection, within a connector, a request should use.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
///
/// # Unknown fields are rejected
///
/// `{"kind": "default", "name": "acme-prod"}` used to parse as [`Self::Default`]
/// and drop `name` on the floor. Nothing downstream can recover the operator's
/// intent from that — the document said "use the connection called
/// `acme-prod`", and the value that reaches the connector says "use whatever
/// single database this connector was configured with", which is a *different
/// physical database* and exactly the mistake `fabric-tenant-runtime`'s
/// `DestinationReuse` then has to catch after the fact.
///
/// A misspelled or misplaced field is a reconciliation fault, and the cheapest
/// place to refuse it is the parse. §28 says fail closed; a silently ignored
/// field is the opposite.
///
/// Note that `deny_unknown_fields` alone does not achieve this — serde applies
/// it only to variants that have fields, so [`Self::Default`] would still have
/// swallowed the surplus. Deserialisation therefore runs through a private
/// mirror type; see `execution/tagged_documents.rs` for the gap and the shape
/// that closes it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(from = "super::tagged_documents::ConnectionSelectorDocument")]
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
    /// For [`Self::Secret`] this is deliberately **not** the reference path.
    /// A reference such as `tenant/acme/data-primary` or
    /// `vault/prod/high-value-client` is safe as an occasional diagnostic
    /// value (§29 permits logging it when resolution genuinely fails — see
    /// [`ConnectorError::SecretUnavailable`](crate::ConnectorError)), but a
    /// telemetry label is attached to *every* request, not an occasional
    /// failure. At that volume the path itself becomes the leak: it spells
    /// out tenant names and vault layout to anything that reads the platform's
    /// traces, which is a broader audience than the operators who are meant to
    /// resolve secrets.
    ///
    /// So this hashes the reference instead. The digest is stable (the same
    /// reference always labels the same way, so traces can still be grouped
    /// and compared) and non-reversible in practice, without the path itself
    /// ever leaving this method. Resolving the secret needs the real path —
    /// use [`Self::secret_reference`] for that, never this label.
    ///
    /// The hash is [`DefaultHasher`], which is unkeyed and whose algorithm is
    /// unspecified across Rust releases. That is fine for a label — it only
    /// has to keep references apart from each other — but it must never be
    /// treated as a security property (a determined party could still brute
    /// force short, guessable references).
    #[must_use]
    pub fn telemetry_label(&self) -> String {
        match self {
            Self::Default => "default".to_owned(),
            Self::Named { name } => format!("named:{name}"),
            Self::Secret { reference } => format!("secret:{:016x}", reference_digest(reference)),
        }
    }

    /// Whether resolving this selector requires a secret lookup.
    #[must_use]
    pub const fn needs_secret(&self) -> bool {
        matches!(self, Self::Secret { .. })
    }

    /// The raw secret reference, for the one caller that must resolve it.
    ///
    /// Everywhere else — logs, spans, telemetry labels — should use
    /// [`Self::telemetry_label`] instead. This accessor exists so the one
    /// legitimate use (handing the reference to a
    /// [`SecretResolver`](crate::SecretResolver)) has a named, greppable call
    /// site rather than a pattern match repeated at every use.
    #[must_use]
    pub const fn secret_reference(&self) -> Option<&SecretRef> {
        match self {
            Self::Secret { reference } => Some(reference),
            Self::Default | Self::Named { .. } => None,
        }
    }
}

/// A short, stable digest of a secret reference, for [`ConnectionSelector::telemetry_label`].
///
/// Not a security primitive — see the warning on that method.
fn reference_digest(reference: &SecretRef) -> u64 {
    let mut hasher = DefaultHasher::new();
    reference.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_telemetry_label_for_a_secret_does_not_contain_the_raw_reference_path() {
        // This is the opposite of what this test used to assert: the label
        // must NOT leak the path, because it now runs on every request.
        let selector = ConnectionSelector::Secret {
            reference: SecretRef::new("tenant/acme/data-primary"),
        };

        let label = selector.telemetry_label();

        assert!(!label.contains("tenant"));
        assert!(!label.contains("acme"));
        assert!(!label.contains("data-primary"));
        assert!(label.starts_with("secret:"));
    }

    #[test]
    fn the_telemetry_label_for_a_secret_is_stable_across_calls() {
        // Traces group and compare by this label, so the same reference must
        // always produce the same opaque label.
        let selector = ConnectionSelector::Secret {
            reference: SecretRef::new("tenant/acme/data-primary"),
        };

        assert_eq!(selector.telemetry_label(), selector.telemetry_label());
    }

    #[test]
    fn the_telemetry_label_for_a_secret_differs_by_reference() {
        // Not a security property (see the doc comment) — just enough to keep
        // two tenants' labels from colliding in the common case.
        let acme = ConnectionSelector::Secret {
            reference: SecretRef::new("tenant/acme/data-primary"),
        };
        let globex = ConnectionSelector::Secret {
            reference: SecretRef::new("tenant/globex/data-primary"),
        };

        assert_ne!(acme.telemetry_label(), globex.telemetry_label());
    }

    #[test]
    fn the_raw_reference_is_still_reachable_through_the_explicit_accessor() {
        let selector = ConnectionSelector::Secret {
            reference: SecretRef::new("tenant/acme/data-primary"),
        };

        assert_eq!(
            selector.secret_reference().map(SecretRef::as_str),
            Some("tenant/acme/data-primary")
        );
        assert_eq!(ConnectionSelector::Default.secret_reference(), None);
    }

    #[test]
    fn only_the_secret_variant_needs_a_lookup() {
        assert!(!ConnectionSelector::Default.needs_secret());
        assert!(ConnectionSelector::Secret {
            reference: SecretRef::new("x")
        }
        .needs_secret());
    }

    #[test]
    fn a_name_supplied_alongside_the_default_kind_is_rejected_not_dropped() {
        // The operator meant `{"kind": "named", "name": "acme-prod"}`. This
        // used to parse as `Default` — "the connector's one database" — which
        // is a claim about infrastructure they never made.
        let error = serde_json::from_str::<ConnectionSelector>(r#"{"kind": "default", "name": "acme-prod"}"#)
            .unwrap_err();

        assert!(error.to_string().contains("name"), "{error}");
    }

    #[test]
    fn a_reference_supplied_alongside_a_named_connection_is_rejected() {
        let error = serde_json::from_str::<ConnectionSelector>(
            r#"{"kind": "named", "name": "acme-prod", "reference": "vault/prod/acme"}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("reference"), "{error}");
    }

    #[test]
    fn the_documents_the_platform_actually_ships_still_parse() {
        for document in [
            r#"{"kind": "default"}"#,
            r#"{"kind": "named", "name": "acme-prod"}"#,
            r#"{"kind": "secret", "reference": "tenant/acme/data-primary"}"#,
        ] {
            assert!(
                serde_json::from_str::<ConnectionSelector>(document).is_ok(),
                "{document}"
            );
        }
    }

    #[test]
    fn every_variant_survives_a_round_trip() {
        // The deny would be a trap if this crate's own `Serialize` output no
        // longer satisfied its `Deserialize`.
        for selector in [
            ConnectionSelector::Default,
            ConnectionSelector::Named {
                name: ConnectionName::try_new("acme-prod").unwrap(),
            },
            ConnectionSelector::Secret {
                reference: SecretRef::new("tenant/acme/data-primary"),
            },
        ] {
            let json = serde_json::to_string(&selector).unwrap();
            let parsed: ConnectionSelector = serde_json::from_str(&json).unwrap();

            assert_eq!(parsed, selector, "{json}");
        }
    }
}
