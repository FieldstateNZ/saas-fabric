//! Which connection, within a connector, a published DataSource selects.

use crate::ConnectionName;

/// The publisher's own declaration of a DataSource's connection selector.
///
/// Mirrors `fabric_connector::ConnectionSelector` — see
/// [`crate::TenantBindingDocument`] for why this crate declares its own copy.
/// `Secret::reference` is a reference **path** only, never a resolved
/// credential. See [`crate::IsolationModelDocument`] for why every variant,
/// including `Default`, is struct-shaped.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConnectionSelectorDocument {
    /// Use the connector's single default connection.
    Default {},

    /// Use a connection the connector already holds configuration for.
    Named {
        /// The connection's name in the connector's configuration.
        name: ConnectionName,
    },

    /// Use a connection built from a credential resolved at execution time.
    Secret {
        /// Where to find the credential — a reference path.
        reference: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_round_trips_through_json() {
        for selector in [
            ConnectionSelectorDocument::Default {},
            ConnectionSelectorDocument::Named {
                name: ConnectionName::try_new("acme-prod").unwrap(),
            },
            ConnectionSelectorDocument::Secret {
                reference: "tenant/acme/data-primary".to_owned(),
            },
        ] {
            let json = serde_json::to_string(&selector).unwrap();
            let parsed: ConnectionSelectorDocument = serde_json::from_str(&json).unwrap();

            assert_eq!(parsed, selector, "{json}");
        }
    }

    #[test]
    fn a_name_supplied_alongside_the_default_kind_is_rejected_not_dropped() {
        let error =
            serde_json::from_str::<ConnectionSelectorDocument>(r#"{"kind": "default", "name": "acme-prod"}"#)
                .unwrap_err();

        assert!(error.to_string().contains("name"), "{error}");
    }
}
