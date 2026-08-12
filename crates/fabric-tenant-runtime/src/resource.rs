//! The reconciled-resource lifecycle, shared by every runtime resource.
//!
//! The runtime plane holds two kinds of reconciled state — tenant bindings and
//! data sources — and they have identical lifecycles: revisioned snapshots,
//! lock-free resolution, revision-guarded application, invalidation, change
//! notification, and a polling refresher over a source.
//!
//! Writing that twice would mean two chances to get the revision guard subtly
//! wrong, and only one of them would have the tests. So it is written once,
//! generically, and each resource type supplies a key and a revision through
//! [`RegistryResource`].

mod apply_report;
mod change;
mod lookup_error;
mod refresher;
mod registry;
mod resource_kind;
mod snapshot;
mod source;
pub mod sources;

pub use apply_report::ApplyReport;
pub use change::{ChangeKind, ResourceChange};
pub use lookup_error::LookupError;
pub use refresher::{RefreshHandle, ResourceRefresher};
pub use registry::ResourceRegistry;
pub use resource_kind::RegistryResource;
pub use source::ResourceSource;
