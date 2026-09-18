//! The one Kubernetes shape this adapter reads and writes.

use std::collections::BTreeMap;

/// A `ConfigMap`, reduced to the fields publication needs.
///
/// `data` only: the documents are UTF-8 JSON, and ADR 0018 rules out
/// `binaryData`. Anything else the API server returns is ignored on read
/// and never sent on write.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ConfigMap {
    /// Name, namespace, labels and the version this object was read at.
    pub(crate) metadata: Metadata,
    /// The document and its manifest, keyed by file name.
    #[serde(default)]
    pub(crate) data: BTreeMap<String, String>,
}

/// The metadata publication reads and writes.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metadata {
    /// The object's name.
    pub(crate) name: String,
    /// The namespace it lives in.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) namespace: String,
    /// Who manages it, so an operator listing the namespace can tell.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) labels: BTreeMap<String, String>,
    /// The version the object was read at; sent back on a replace so the
    /// API server refuses a write over a version this adapter did not see.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resource_version: Option<String>,
}

/// The label every object this adapter writes carries.
pub(crate) const MANAGED_BY_LABEL: (&str, &str) = ("app.kubernetes.io/managed-by", "saas-fabric");
