//! One tenant's complete runtime binding, as the publisher declares it.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, TenantId};

use crate::canonical::to_canonical_bytes;
use crate::{ConfigurationBindingDocument, StorageBindingDocument, TenantDataBindings};

/// The publisher's own declaration of one tenant's runtime binding.
///
/// This mirrors `fabric_tenant_runtime::TenantRuntimeBinding` field for
/// field, but it is a **separate type**: this crate may not depend on
/// `fabric-tenant-runtime` (see `docs/architecture/crate-dependencies.md`),
/// so it cannot reuse that type directly. Fidelity between the two copies is
/// guaranteed by `#[serde(deny_unknown_fields)]` on the consumer's side, plus
/// the round-trip test beside this type, which deserialises this crate's own
/// canonical JSON as the consumer's `TenantRuntimeBinding`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantBindingDocument {
    /// Which tenant this describes.
    pub tenant: TenantId,

    /// The revision of this tenant's binding — a resource revision, and
    /// independent of the document's own [`crate::DocumentRevision`].
    pub revision: BindingRevision,

    /// Logical data source name to the DataSource this tenant is bound to.
    ///
    /// Non-empty whenever this crate builds one — see
    /// [`TenantDataBindings`] for why an empty map is still accepted here on
    /// the way in.
    #[serde(default = "TenantDataBindings::empty_for_deserialisation")]
    pub data: TenantDataBindings,

    /// Where this tenant's configuration lives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<ConfigurationBindingDocument>,

    /// The base secret **path** for this tenant, such as
    /// `vault/tenants/acme` — never a resolved value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secrets: Option<String>,

    /// Feature flags in force for this tenant.
    #[serde(default)]
    pub features: BTreeMap<String, bool>,

    /// Named storage areas.
    #[serde(default)]
    pub storage: BTreeMap<String, StorageBindingDocument>,
}

/// Renders a tenants document: every binding, sorted by tenant so an
/// unrelated edit produces no diff, then rendered as canonical JSON
/// (two-space indentation, a trailing newline, UTF-8).
///
/// # Errors
///
/// Returns [`serde_json::Error`] only if a value's own `Serialize`
/// implementation fails, which cannot happen for this crate's validated
/// types.
pub fn tenants_canonical_json(bindings: &[TenantBindingDocument]) -> Result<Vec<u8>, serde_json::Error> {
    let mut sorted = bindings.to_vec();
    sorted.sort_by(|left, right| left.tenant.cmp(&right.tenant));
    to_canonical_bytes(&sorted)
}

#[cfg(test)]
mod tests {
    use fabric_core::LogicalDataSourceName;
    use fabric_tenant_runtime::TenantRuntimeBinding;

    use super::*;
    use crate::{IsolationModelDocument, TenantDataBindingDocument};

    fn acme_isolated_by(isolation: IsolationModelDocument) -> TenantBindingDocument {
        let mut data = BTreeMap::new();
        data.insert(
            LogicalDataSourceName::try_new("primary").unwrap(),
            TenantDataBindingDocument {
                data_source: fabric_core::DataSourceId::try_new("sql-au-east-03").unwrap(),
                isolation,
            },
        );

        TenantBindingDocument {
            tenant: TenantId::try_new("acme").unwrap(),
            revision: BindingRevision::new(42),
            data: TenantDataBindings::try_new(data).unwrap(),
            configuration: None,
            secrets: Some("vault/tenants/acme".to_owned()),
            features: BTreeMap::from([("invoicing".to_owned(), true)]),
            storage: BTreeMap::new(),
        }
    }

    fn acme() -> TenantBindingDocument {
        acme_isolated_by(IsolationModelDocument::Database {})
    }

    #[test]
    fn a_published_tenant_document_deserialises_as_the_runtimes_own_binding() {
        // Every isolation variant, not just the one `Database` happens to
        // cover -- a discriminator's `column`/`value` and a schema's
        // `schema` are exactly the fields most likely to drift from the
        // consumer's own shape.
        for isolation in [
            IsolationModelDocument::Database {},
            IsolationModelDocument::Schema {
                schema: crate::SchemaName::try_new("acme").unwrap(),
            },
            IsolationModelDocument::Discriminator {
                column: crate::FieldName::try_new("tenant_key").unwrap(),
                value: "tenant-482".to_owned(),
            },
        ] {
            let binding = acme_isolated_by(isolation.clone());
            let bytes = tenants_canonical_json(std::slice::from_ref(&binding)).unwrap();

            let bindings: Vec<TenantRuntimeBinding> = serde_json::from_slice(&bytes).unwrap();

            assert_eq!(bindings.len(), 1, "{isolation:?}");
            assert_eq!(bindings[0].tenant.as_str(), "acme", "{isolation:?}");
        }
    }

    #[test]
    fn a_field_the_runtime_does_not_know_is_rejected_rather_than_ignored() {
        let mut value = serde_json::to_value(acme()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpectedField".to_owned(), serde_json::Value::Bool(true));

        let result: Result<TenantRuntimeBinding, _> = serde_json::from_value(value);

        assert!(result.is_err());
    }
}
