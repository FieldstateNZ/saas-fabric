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

use super::held::read_held;
use super::paths::DocumentPaths;
use super::write::write_if_needed;
use crate::{
    plan_publication, DocumentKind, DocumentPlan, PublicationError, PublicationReport, PublishedRevisions,
    RuntimePublication, RuntimeSnapshot,
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
        Ok(read_held(&self.tenants, &self.data_sources, &self.catalog)?.revisions())
    }

    async fn publish(&self, snapshot: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
        let held = read_held(&self.tenants, &self.data_sources, &self.catalog)?;
        let plan = plan_publication(snapshot, &held)?;

        // The order ADR 0018 part 3 requires: a data source before anything
        // that names it, and tenants last.
        write(&self.data_sources, &plan.data_sources)?;
        write(&self.catalog, &plan.catalog)?;
        write(&self.tenants, &plan.tenants)?;

        Ok(PublicationReport {
            tenants: plan.tenants.outcome,
            data_sources: plan.data_sources.outcome,
            catalog: plan.catalog.outcome,
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

fn write(paths: &DocumentPaths, plan: &DocumentPlan) -> Result<(), PublicationError> {
    write_if_needed(paths, plan.outcome, &plan.bytes, plan.revision)
}
