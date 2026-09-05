//! What each document's publication must do, and the exact canonical bytes
//! it would write — computed once, before any write happens.

use super::held::HeldState;
use crate::verdict::{verdict, Incoming, Verdict};
use crate::{
    data_sources_canonical_json, tenants_canonical_json, DocumentKind, PublicationError, RuntimeSnapshot,
};

/// One document's resolved verdict, paired with the canonical bytes that
/// verdict was computed against.
pub(super) struct DocumentPlan {
    pub(super) verdict: Verdict,
    pub(super) bytes: Vec<u8>,
}

/// All three documents' plans. Resolving every verdict here, before
/// [`super::adapter::FilesystemRuntimePublication`] writes anything, is what
/// makes "compute every document's verdict, and only then write" (ADR 0018
/// parts 4-6) true in code rather than just in prose.
pub(super) struct PublishPlan {
    pub(super) data_sources: DocumentPlan,
    pub(super) catalog: DocumentPlan,
    pub(super) tenants: DocumentPlan,
}

impl PublishPlan {
    /// Resolves every document's verdict against `held`. Returns the first
    /// error encountered; no write has happened yet either way.
    ///
    /// # Errors
    ///
    /// A document's [`PublicationError::StaleRevision`] or
    /// [`PublicationError::DivergentPayload`], or
    /// [`PublicationError::Unwritable`] if a document's own `Serialize`
    /// implementation failed — which none of this crate's validated types
    /// ever do.
    pub(super) fn build(snapshot: &RuntimeSnapshot, held: &HeldState) -> Result<Self, PublicationError> {
        let data_sources_bytes = data_sources_canonical_json(&snapshot.data_sources.payload)
            .map_err(|cause| unwritable(DocumentKind::DataSources, cause))?;
        let catalog_bytes = snapshot
            .catalog
            .payload
            .canonical_json()
            .map_err(|cause| unwritable(DocumentKind::Catalog, cause))?;
        let tenants_bytes = tenants_canonical_json(&snapshot.tenants.payload)
            .map_err(|cause| unwritable(DocumentKind::Tenants, cause))?;

        let data_sources_verdict = verdict(
            held.data_sources_held(),
            &Incoming {
                document: DocumentKind::DataSources,
                revision: snapshot.data_sources.revision,
                payload: &data_sources_bytes,
            },
        )?;
        let catalog_verdict = verdict(
            held.catalog_held(),
            &Incoming {
                document: DocumentKind::Catalog,
                revision: snapshot.catalog.revision,
                payload: &catalog_bytes,
            },
        )?;
        let tenants_verdict = verdict(
            held.tenants_held(),
            &Incoming {
                document: DocumentKind::Tenants,
                revision: snapshot.tenants.revision,
                payload: &tenants_bytes,
            },
        )?;

        Ok(Self {
            data_sources: DocumentPlan {
                verdict: data_sources_verdict,
                bytes: data_sources_bytes,
            },
            catalog: DocumentPlan {
                verdict: catalog_verdict,
                bytes: catalog_bytes,
            },
            tenants: DocumentPlan {
                verdict: tenants_verdict,
                bytes: tenants_bytes,
            },
        })
    }
}

fn unwritable(
    document: DocumentKind,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> PublicationError {
    PublicationError::Unwritable {
        document,
        cause: Box::new(cause),
    }
}
