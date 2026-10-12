//! The one pointer a published runtime resolves through.

use arc_swap::ArcSwapOption;
use fabric_core::ReaderGenerationReport;

use super::resolution_view::ResolutionView;
use crate::publication::DecodedCatalogue;
use crate::{DataSourceRegistry, TenantRegistry};

/// The catalogue, tenant and DataSource snapshots of the last complete
/// generation, swapped in together.
///
/// The two registries are written one after the other: data sources, then
/// tenants. Resolving across them directly would, between those two writes,
/// pair the new DataSources with the previous tenant bindings. When a
/// generation rebinds tenants to DataSources whose connections have also
/// moved, that pairing resolves a tenant to another tenant's database -- a
/// complete, successful, cross-tenant execution target.
///
/// So a published runtime does not resolve across the registries. The
/// refresher writes both, then [`Self::publish`]es what they hold as one
/// [`ResolutionView`] in a single atomic store, and the resolver reads only
/// that. Between the registry writes, requests keep resolving against the
/// previous view whole; after the store, against the new one whole.
#[derive(Debug, Default)]
pub(crate) struct PublishedGeneration {
    current: ArcSwapOption<ResolutionView>,
}

impl PublishedGeneration {
    /// Installs `catalogue` and what both registries now hold as the view
    /// every request reads, stamped with `served`: the generation all three
    /// are exactly, if they are. The stamp is installed in the same store as
    /// the snapshots, so what a replica reports serving is always what its
    /// requests read, catalogue included (G7, G8).
    ///
    /// Called only once both registries have been written for a generation,
    /// and only by the refresher that wrote them and decoded `catalogue` --
    /// so all three it pairs are always the same generation's.
    pub(crate) fn publish(
        &self,
        catalogue: DecodedCatalogue,
        tenants: &TenantRegistry,
        data_sources: &DataSourceRegistry,
        served: Option<ReaderGenerationReport>,
    ) {
        self.current.store(Some(std::sync::Arc::new(ResolutionView::of(
            Some(catalogue),
            tenants,
            data_sources,
            served,
        ))));
    }

    /// The view installed by the last [`Self::publish`]; an unprimed view
    /// before the first.
    pub(crate) fn view(&self) -> ResolutionView {
        self.current.load().as_deref().cloned().unwrap_or_default()
    }
}
