//! The `RuntimePublication` port, backed by three files and their sidecar
//! manifests on a local filesystem.
//!
//! This file is a single `impl RuntimePublication`, and `publish` alone
//! touches all three documents in a fixed order — splitting `current` or
//! `publish` out of this `impl` block would separate the trait's methods
//! from each other for no reason a reader could act on. The read, parse,
//! validate, plan, and write steps each already have their own file
//! (`held`, `parse`, `validate`, `plan`, `write`, `atomic_write`).

use std::path::PathBuf;

use async_trait::async_trait;

use super::held::HeldState;
use super::parse::{parse_documents, parse_held_tenants};
use super::paths::DocumentPaths;
use super::plan::PublishPlan;
use super::write::write_if_needed;
use crate::validate::validate_snapshot;
use crate::{
    DataSourceDocument, DocumentKind, PublicationError, PublicationReport, PublishedRevisions,
    RuntimePublication, RuntimeSnapshot, TenantBindingDocument,
};

/// Publishes the runtime's three documents to a local filesystem, matching
/// the layout `saas-fabric-platform` mounts today: three payload files, each
/// with a sidecar manifest beside it (ADR 0018, "The production owner").
///
/// This is not the Kubernetes adapter — see ADR 0018, "The Kubernetes
/// adapter", for that design, which relies on the kubelet's own atomic
/// symlink swap instead of `rename(2)` but keeps the same payload-before-
/// manifest ordering.
pub struct FilesystemRuntimePublication {
    tenants: DocumentPaths,
    data_sources: DocumentPaths,
    catalog: DocumentPaths,
}

impl FilesystemRuntimePublication {
    /// Publishes to the three given payload paths — the runtime's own
    /// `tenants_path`, `data_sources_path`, and `catalog_path`. Each
    /// document's manifest is derived from its payload's parent directory
    /// and this crate's own manifest file name for that document.
    #[must_use]
    pub fn new(
        tenants_path: impl Into<PathBuf>,
        data_sources_path: impl Into<PathBuf>,
        catalog_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            tenants: DocumentPaths::new(DocumentKind::Tenants, tenants_path.into()),
            data_sources: DocumentPaths::new(DocumentKind::DataSources, data_sources_path.into()),
            catalog: DocumentPaths::new(DocumentKind::Catalog, catalog_path.into()),
        }
    }
}

#[async_trait]
impl RuntimePublication for FilesystemRuntimePublication {
    async fn current(&self) -> Result<PublishedRevisions, PublicationError> {
        let held = HeldState::read(&self.tenants, &self.data_sources, &self.catalog)?;

        Ok(PublishedRevisions {
            tenants: held.tenants_held().map(|held| held.revision),
            data_sources: held.data_sources_held().map(|held| held.revision),
            catalog: held.catalog_held().map(|held| held.revision),
        })
    }

    async fn publish(&self, snapshot: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
        // Read every held fact before a single byte is written (ADR 0018
        // parts 4-6): validation and every document's verdict both compare
        // against what is currently on disk.
        let held = HeldState::read(&self.tenants, &self.data_sources, &self.catalog)?;

        let held_tenants: Vec<TenantBindingDocument> = parse_held_tenants(
            held.tenants_manifest.as_ref(),
            held.tenants_payload.as_deref(),
            self.tenants.kind,
        )?;
        let held_data_sources: Vec<DataSourceDocument> =
            parse_documents(held.data_sources_payload.as_deref(), self.data_sources.kind)?;
        validate_snapshot(snapshot, &held_tenants, &held_data_sources)?;

        let plan = PublishPlan::build(snapshot, &held)?;

        // Only now does the first byte get written. Data sources, then the
        // catalogue, then tenants (ADR 0018 part 3): additions must land
        // before anything can reference them.
        write_if_needed(
            &self.data_sources,
            plan.data_sources.verdict,
            &plan.data_sources.bytes,
            snapshot.data_sources.revision,
        )?;
        write_if_needed(
            &self.catalog,
            plan.catalog.verdict,
            &plan.catalog.bytes,
            snapshot.catalog.revision,
        )?;
        write_if_needed(
            &self.tenants,
            plan.tenants.verdict,
            &plan.tenants.bytes,
            snapshot.tenants.revision,
        )?;

        Ok(PublicationReport {
            tenants: plan.tenants.verdict.into(),
            data_sources: plan.data_sources.verdict.into(),
            catalog: plan.catalog.verdict.into(),
        })
    }

    fn describe(&self) -> String {
        format!(
            "filesystem: tenants={}, data_sources={}, catalog={}",
            self.tenants.payload.display(),
            self.data_sources.payload.display(),
            self.catalog.payload.display()
        )
    }
}
