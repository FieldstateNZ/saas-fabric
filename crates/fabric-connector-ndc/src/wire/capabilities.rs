//! `GET /capabilities` response types.

use serde_json::Value;

/// The body of `GET /capabilities`.
///
/// The nested capability objects are kept as raw JSON. NDC signals an optional
/// capability by the *presence of a non-null key* rather than a boolean, and
/// the set grows between specification versions — modelling each one would mean
/// a struct that fails to deserialise every time Hasura adds a field. Such
/// checks are both sufficient and forward-compatible.
///
/// Note the "non-null" half, which is easy to lose. Every optional capability
/// is declared `anyOf [LeafCapability, null]`, so an explicit `null` is a
/// conforming way of saying *unsupported* — and `Value::get` answers
/// `Some(Value::Null)` for it, which a bare `is_some()` reads as support.
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
    ///
    /// Presence **and** non-null — see the type docs for why the second half
    /// is not optional. Reading `{"transactional": null}` as support would
    /// have the platform advertise transactional writes that the connector
    /// then does not provide, and a caller relying on all-or-nothing would get
    /// partial writes on failure with nothing in the response saying so.
    pub(crate) fn supports_transactional_mutations(&self) -> bool {
        self.capabilities
            .get("mutation")
            .and_then(|mutation| mutation.get("transactional"))
            .is_some_and(|capability| !capability.is_null())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(json: &str) -> NdcCapabilitiesResponse {
        serde_json::from_str(json).unwrap()
    }

    /// Reads a real `ndc-postgres` v3.1.0 document, checked in under
    /// `tests/fixtures/` -- see the README there for how it was captured.
    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/ndc-postgres-v3.1.0/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
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
    fn an_explicit_null_capability_means_unsupported_not_supported() {
        // `anyOf [LeafCapability, null]`, so this is a conforming way of
        // saying no. `Value::get` returns `Some(Null)`, which is what made a
        // bare presence check read it as yes.
        let parsed = response(r#"{"version":"0.2.13","capabilities":{"mutation":{"transactional":null}}}"#);

        assert!(!parsed.supports_transactional_mutations());
    }

    #[test]
    fn an_unrecognised_capability_key_does_not_break_deserialisation() {
        // Forward compatibility: a newer connector advertising something we
        // have never heard of must still be usable.
        let parsed = response(r#"{"version":"0.2.13","capabilities":{"query":{"time_travel":{}}}}"#);

        assert_eq!(parsed.version, "0.2.13");
    }

    #[test]
    fn the_real_connector_declares_transactional_mutations() {
        // `ghcr.io/hasura/ndc-postgres:v3.1.0@sha256:f91910ef5107aa80d31d82639e149b7f41f4a5bb3af9a369397d7d5965d79a57`,
        // `GET /capabilities`. `mutation.transactional` is present as `{}`,
        // not absent and not `null` -- the one shape this method reads as
        // "yes".
        let parsed = response(&fixture("capabilities.json"));

        assert_eq!(parsed.version, "0.2.4");
        assert!(parsed.supports_transactional_mutations());
    }
}
