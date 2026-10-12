//! The resource rules a publication holds its own documents to, applied to
//! held bytes: a digest that matches the record says the bytes are the ones
//! the record names, not that they are usable.
//!
//! Every problem names resources by their position in the document, never
//! by an identifier or any other held value.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): the
//! resource rules, each small, kept together so both callers share them.

use std::collections::{BTreeMap, BTreeSet};

use super::held_parse::sanitized;
use crate::digest::sha256_hex;
use crate::{
    DataSourceDocument, IssuerRegistryDocument, RecordedRegistry, TenantBindingDocument,
    REGISTRY_SCHEMA_VERSION,
};

/// The index of each item whose key an earlier item already had.
fn repeats<'a, K: Ord + 'a>(keys: impl Iterator<Item = &'a K>) -> Vec<(usize, usize)> {
    let mut first = BTreeMap::new();
    keys.enumerate()
        .filter_map(|(index, key)| {
            if let Some(&earlier) = first.get(key) {
                return Some((earlier, index));
            }
            first.insert(key, index);
            None
        })
        .collect()
}

/// A tenant bound twice, or with no data binding.
pub(super) fn tenant_problems(tenants: &[TenantBindingDocument]) -> Vec<String> {
    let mut problems: Vec<String> = repeats(tenants.iter().map(|tenant| &tenant.tenant))
        .into_iter()
        .map(|(earlier, index)| {
            format!("tenant at position {index} is bound more than once (first at {earlier})")
        })
        .collect();
    problems.extend(
        tenants
            .iter()
            .enumerate()
            .filter(|(_, tenant)| tenant.data.is_empty())
            .map(|(index, _)| format!("tenant at position {index} has no data binding")),
    );
    problems
}

/// A DataSource declared twice.
pub(super) fn data_source_problems(data_sources: &[DataSourceDocument]) -> Vec<String> {
    repeats(data_sources.iter().map(|data_source| &data_source.id))
        .into_iter()
        .map(|(earlier, index)| {
            format!("data source at position {index} is declared more than once (first at {earlier})")
        })
        .collect()
}

/// A tenant binding that names a DataSource the data-sources document lacks.
pub(super) fn dangling(
    tenants: &[TenantBindingDocument],
    data_sources: &[DataSourceDocument],
) -> Vec<String> {
    let declared: BTreeSet<_> = data_sources.iter().map(|data_source| &data_source.id).collect();
    tenants
        .iter()
        .enumerate()
        .filter(|(_, tenant)| {
            tenant
                .data
                .values()
                .any(|binding| !declared.contains(&binding.data_source))
        })
        .map(|(index, _)| format!("tenant at position {index} binds a data source that is not declared"))
        .collect()
}

/// What is wrong with held registry bytes the record names: a digest,
/// generation or schema other than the recorded one, an issuer registered
/// twice, or, when the tenants are readable, an issuer for an unbound tenant.
pub(super) fn registry_problems(
    bytes: Option<&[u8]>,
    recorded: &RecordedRegistry,
    tenants: Option<&[TenantBindingDocument]>,
) -> Vec<String> {
    let Some(bytes) = bytes else {
        return vec!["the record names an issuer registry, but none is held".to_owned()];
    };
    if sha256_hex(bytes) != recorded.sha256 {
        return vec!["the held issuer registry is not the bytes the record names".to_owned()];
    }
    let registry = match serde_json::from_slice::<IssuerRegistryDocument>(bytes) {
        Ok(registry) => registry,
        Err(error) => {
            return vec![format!(
                "the issuer registry does not parse: {}",
                sanitized(&error)
            )]
        }
    };
    let mut problems = Vec::new();
    if registry.schema_version() != REGISTRY_SCHEMA_VERSION {
        problems.push(format!(
            "the issuer registry is schema {}, not {REGISTRY_SCHEMA_VERSION}",
            registry.schema_version()
        ));
    }
    if registry.generation() != recorded.generation {
        problems.push("the issuer registry's generation is not the recorded one".to_owned());
    }
    let issuers = registry.issuers();
    problems.extend(
        repeats(issuers.iter().map(|registration| &registration.issuer))
            .into_iter()
            .map(|(earlier, index)| {
                format!("issuer at position {index} is registered more than once (first at {earlier})")
            }),
    );
    if let Some(tenants) = tenants {
        let bound: BTreeSet<_> = tenants.iter().map(|tenant| &tenant.tenant).collect();
        problems.extend(
            issuers
                .iter()
                .enumerate()
                .filter(|(_, registration)| !bound.contains(&registration.tenant))
                .map(|(index, _)| {
                    format!("issuer at position {index} is registered to a tenant that is not bound")
                }),
        );
    }
    problems
}
