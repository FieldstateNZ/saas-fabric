//! A complete generation, decoded, and whether it can be served whole.

use std::collections::BTreeSet;

use super::complete_generation::CompleteGeneration;
use super::file_reads::parse;
use super::CatalogueCheck;
use crate::resource::RegistryResource;
use crate::{DataSource, SourceError, TenantRuntimeBinding};

/// A [`CompleteGeneration`] and its two documents, decoded.
#[derive(Debug, Clone)]
pub(crate) struct ParsedGeneration {
    pub(crate) complete: CompleteGeneration,
    pub(crate) data_sources: Vec<DataSource>,
    pub(crate) tenants: Vec<TenantRuntimeBinding>,
}

impl ParsedGeneration {
    /// Decodes the documents, as a reader without a last-known-good copy
    /// always has: a resource that then fails its own validation is left to
    /// the registry, which keeps any copy it holds.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] if either document does not decode.
    pub(crate) fn parse(complete: CompleteGeneration, origin: &str) -> Result<Self, SourceError> {
        Ok(Self {
            data_sources: parse(&complete.data_sources, origin)?,
            tenants: parse(&complete.tenants, origin)?,
            complete,
        })
    }

    /// Decodes and checks everything a replica would need to serve this
    /// generation whole, and start again from it: both documents through
    /// the real decoders, every resource's own validation, no key twice in
    /// one document, no binding to a DataSource the generation does not
    /// carry, and the catalogue through the host's decoder when it was read.
    ///
    /// A reader with a last-known-good copy uses this before it chooses a
    /// generation, raises its floor, or persists anything, so a generation
    /// that would fail to serve can never displace one that serves.
    ///
    /// # Errors
    ///
    /// [`SourceError::Unusable`] naming the document and the reason.
    pub(crate) fn validate(
        complete: CompleteGeneration,
        catalogue: &dyn CatalogueCheck,
        origin: &str,
    ) -> Result<Self, SourceError> {
        let generation = complete.generation;
        let unusable = |document: &'static str, reason: String| SourceError::Unusable {
            origin: origin.to_owned(),
            generation,
            document,
            reason,
        };
        let data_sources: Vec<DataSource> = parse(&complete.data_sources, origin)
            .map_err(|error| unusable("data-sources", error.to_string()))?;
        let tenants: Vec<TenantRuntimeBinding> =
            parse(&complete.tenants, origin).map_err(|error| unusable("tenants", error.to_string()))?;
        each_valid_once(&data_sources).map_err(|reason| unusable("data-sources", reason))?;
        each_valid_once(&tenants).map_err(|reason| unusable("tenants", reason))?;
        every_binding_resolves(&tenants, &data_sources).map_err(|reason| unusable("tenants", reason))?;
        if let Some(bytes) = &complete.catalog {
            catalogue
                .check(bytes)
                .map_err(|reason| unusable("catalog", reason))?;
        }
        Ok(Self {
            complete,
            data_sources,
            tenants,
        })
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.complete.generation
    }
}

fn each_valid_once<T: RegistryResource>(resources: &[T]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for resource in resources {
        resource
            .validate()
            .map_err(|error| format!("{} {}: {error}", T::KIND, resource.key()))?;
        if !seen.insert(resource.key().clone()) {
            return Err(format!("{} {} appears twice", T::KIND, resource.key()));
        }
    }
    Ok(())
}

fn every_binding_resolves(
    tenants: &[TenantRuntimeBinding],
    data_sources: &[DataSource],
) -> Result<(), String> {
    let carried: BTreeSet<_> = data_sources.iter().map(|source| &source.id).collect();
    for tenant in tenants {
        for (logical, binding) in &tenant.data {
            if !carried.contains(&binding.data_source) {
                return Err(format!(
                    "tenant {} binds {logical} to data source {}, which this generation does not carry",
                    tenant.tenant, binding.data_source
                ));
            }
        }
    }
    Ok(())
}
