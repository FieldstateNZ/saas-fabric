//! `GET /capabilities` response types.

use serde_json::Value;

/// The body of `GET /capabilities`.
///
/// The nested capability objects are kept as raw JSON. NDC signals an optional
/// capability by the *presence* of a key rather than a boolean, and the set
/// grows between specification versions — modelling each one would mean a
/// struct that fails to deserialise every time Hasura adds a field. Presence
/// checks are both sufficient and forward-compatible.
///
/// Note what is **not** here: filtering, ordering, and paging. Those are core
/// NDC, required of every conforming connector, so there is nothing to
/// negotiate.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcCapabilitiesResponse {
    /// The specification version the connector implements.
    pub(crate) version: String,

    /// The capability tree.
    #[serde(default)]
    pub(crate) capabilities: Value,
}

impl NdcCapabilitiesResponse {
    /// Whether the connector groups several mutations into one transaction.
    pub(crate) fn supports_transactional_mutations(&self) -> bool {
        self.capabilities
            .get("mutation")
            .and_then(|mutation| mutation.get("transactional"))
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(json: &str) -> NdcCapabilitiesResponse {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn detects_transactional_mutation_support_by_key_presence() {
        let parsed = response(r#"{"version":"0.2.13","capabilities":{"mutation":{"transactional":{}}}}"#);

        assert!(parsed.supports_transactional_mutations());
    }

    #[test]
    fn a_connector_without_transactional_mutations_reports_false() {
        let parsed = response(r#"{"version":"0.2.13","capabilities":{"mutation":{}}}"#);

        assert!(!parsed.supports_transactional_mutations());
    }

    #[test]
    fn an_unrecognised_capability_key_does_not_break_deserialisation() {
        // Forward compatibility: a newer connector advertising something we
        // have never heard of must still be usable.
        let parsed = response(r#"{"version":"0.2.13","capabilities":{"query":{"time_travel":{}}}}"#);

        assert_eq!(parsed.version, "0.2.13");
    }
}
