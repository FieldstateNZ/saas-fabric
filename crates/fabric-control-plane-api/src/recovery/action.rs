//! What the operator asked the recovery command for.

use std::path::PathBuf;

use fabric_runtime_publication::DocumentKind;

/// Where the held documents are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The filesystem adapter's three payload paths.
    Filesystem {
        /// `tenants.json`; the record and the lock sit beside it.
        tenants: PathBuf,
        /// `data-sources.json`.
        data_sources: PathBuf,
        /// `catalog.json`.
        catalog: PathBuf,
    },
    /// The Kubernetes adapter's `ConfigMap`s, read with the pod's identity.
    Kubernetes {
        /// The namespace they live in.
        namespace: String,
    },
}

/// What the operator asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Report what is held and what blocks publication. Writes nothing.
    Inspect {
        /// Where.
        target: Target,
    },
    /// As `Inspect`, exiting non-zero when anything blocks publication.
    Validate {
        /// Where.
        target: Target,
    },
    /// Put back the bytes the held record names for one document.
    Restore {
        /// Where.
        target: Target,
        /// Tenants or data sources.
        document: DocumentKind,
        /// The file holding the bytes to put back.
        from: PathBuf,
        /// The audit log to append to before writing.
        audit: PathBuf,
        /// Who, recorded.
        operator: String,
        /// Why, recorded.
        reason: String,
    },
}
