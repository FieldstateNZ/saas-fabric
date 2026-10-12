//! A complete generation, decoded, and whether it can be served whole.
//!
//! In the 121-150 line band: one type, its two decoders (the plain one and
//! the whole-generation validation) and the private rules both share.

use std::collections::BTreeSet;

use super::complete_generation::CompleteGeneration;
use super::file_reads::parse;
use super::{CatalogueDecoder, DecodedCatalogue};
use crate::resource::RegistryResource;
use crate::{DataSource, SourceError, TenantRuntimeBinding};

/// A [`CompleteGeneration`] and its three documents, decoded.
#[derive(Debug, Clone)]
pub(crate) struct ParsedGeneration {
    pub(crate) complete: CompleteGeneration,
    pub(crate) data_sources: Vec<DataSource>,
    pub(crate) tenants: Vec<TenantRuntimeBinding>,
    pub(crate) catalogue: DecodedCatalogue,
}

impl ParsedGeneration {
    /// Decodes the documents, as a reader without a last-known-good copy
    /// does: a resource that then fails its own validation is judged by the
    /// refresher before anything is installed. The catalogue goes through
    /// the host's decoder, as it does with a copy.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] if either runtime document does not
    /// decode, and [`SourceError::Unusable`] naming `catalog` if the host
    /// cannot decode the catalogue.
    pub(crate) fn parse(
        complete: CompleteGeneration,
        decoder: &dyn CatalogueDecoder,
        origin: &str,
    ) -> Result<Self, SourceError> {
        Ok(Self {
            data_sources: parse(&complete.data_sources, origin)?,
            tenants: parse(&complete.tenants, origin)?,
            catalogue: decode_catalogue(&complete, decoder, origin)?,
            complete,
        })
    }

    /// Decodes and checks everything a replica would need to serve this
    /// generation whole, and start again from it: both documents through
    /// the real decoders, every resource's own validation, no key twice in
    /// one document, no binding to a DataSource the generation does not
    /// carry, and the catalogue through the host's decoder.
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
        decoder: &dyn CatalogueDecoder,
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
        let catalogue = decode_catalogue(&complete, decoder, origin)?;
        Ok(Self {
            complete,
            data_sources,
            tenants,
            catalogue,
        })
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.complete.generation
    }
}

fn decode_catalogue(
    complete: &CompleteGeneration,
    decoder: &dyn CatalogueDecoder,
    origin: &str,
) -> Result<DecodedCatalogue, SourceError> {
    decoder
        .decode(&complete.catalog)
        .map_err(|reason| SourceError::Unusable {
            origin: origin.to_owned(),
            generation: complete.generation,
            document: "catalog",
            reason,
        })
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
