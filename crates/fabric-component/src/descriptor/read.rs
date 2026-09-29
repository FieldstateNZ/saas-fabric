//! Reading a component descriptor strictly: bounded, UTF-8, no repeated
//! key, the right envelope, then the spec, then its rules.
use super::constants::{family_version, API_VERSION, MAX_DOCUMENT_BYTES};
use super::{envelope, strict_json, ComponentDescriptor, ComponentSpec};
use crate::errors::{invalid, ContractError};

impl ComponentDescriptor {
    /// Reads a component descriptor from its published bytes.
    ///
    /// In order, the first failure deciding: more than
    /// [`MAX_DOCUMENT_BYTES`] is refused before anything is parsed; so is
    /// text that is not UTF-8, or JSON with a key repeated in any object;
    /// then the envelope must be v1's, before the `spec` is parsed, which
    /// refuses unknown fields; then [`validate`](Self::validate).
    ///
    /// # Errors
    ///
    /// Returns [`ContractError::UnsupportedVersion`] for a `Component` of
    /// another `apiVersion` in this group, naming the version after the
    /// group (`v2`), and
    /// [`ContractError::Invalid`] for everything else.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ContractError> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid(format!(
                "A component descriptor is at most {MAX_DOCUMENT_BYTES} bytes; this one is {}",
                bytes.len()
            )));
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| invalid("A component descriptor must be UTF-8 text"))?;
        strict_json::refuse_duplicate_keys(text)?;
        let value: serde_json::Value = serde_json::from_str(text).map_err(|error| malformed(&error))?;
        let string_at = |key: &str| value.get(key).and_then(serde_json::Value::as_str);
        envelope::check(string_at("apiVersion"), string_at("kind"))?;
        let document: envelope::Read<ComponentSpec> =
            serde_json::from_value(value).map_err(|error| malformed(&error))?;
        Self::new(document.spec)
    }

    /// Reads a component descriptor from an OCI artifact: its
    /// `artifactType` and its one layer's bytes.
    ///
    /// The artifact type must be of the family and of version 1, and the
    /// document's `apiVersion` must agree with it (ADR 0026 section 2).
    ///
    /// # Errors
    ///
    /// Returns [`ContractError::UnsupportedVersion`] for an artifact type of
    /// the family at another version, naming it as `v2` is named, the
    /// spelling [`from_json`](Self::from_json) uses for an `apiVersion`; and
    /// [`ContractError::Invalid`] for a type outside the family, a document
    /// whose `apiVersion` disagrees, or anything [`from_json`](Self::from_json)
    /// refuses.
    pub fn from_artifact(artifact_type: &str, bytes: &[u8]) -> Result<Self, ContractError> {
        let Some(version) = family_version(artifact_type) else {
            return Err(invalid(format!(
                "{artifact_type} is not a component descriptor's artifact type"
            )));
        };
        let expected = format!("v{version}");
        if version != "1" {
            return Err(ContractError::UnsupportedVersion { found: expected });
        }
        let agrees = API_VERSION
            .rsplit_once('/')
            .is_some_and(|(_, own)| own == expected);
        match Self::from_json(bytes) {
            Ok(descriptor) if agrees => Ok(descriptor),
            Ok(_) => Err(disagreement(API_VERSION, artifact_type)),
            Err(ContractError::UnsupportedVersion { found }) => Err(disagreement(&found, artifact_type)),
            Err(error) => Err(error),
        }
    }
}

/// A document whose `apiVersion` is not its artifact type's.
fn disagreement(api_version: &str, artifact_type: &str) -> ContractError {
    invalid(format!(
        "The component descriptor's apiVersion {api_version} disagrees with its artifact type {artifact_type}"
    ))
}

/// A document `serde_json` could not read as a component descriptor.
fn malformed(error: &serde_json::Error) -> ContractError {
    invalid(format!("The component descriptor could not be read: {error}"))
}
