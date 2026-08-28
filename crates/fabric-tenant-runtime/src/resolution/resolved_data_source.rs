//! The end of the resolution chain.

use std::sync::Arc;

use fabric_connector::ExecutionTarget;

use crate::DataSource;

/// A tenant's logical data source, fully resolved.
///
/// Carries two things, because callers need both and neither implies the other:
///
/// - **`target`** — where to execute. Everything a connector needs, and nothing
///   more.
/// - **`data_source`** — the DataSource resource it came from, so the caller
///   can check what the platform permits before executing. A read-only replica
///   is refused here rather than at some depth inside a vendor's error message.
///
/// # This does not cross the Data API boundary
///
/// `data_source` is placement detail. It is read by the execution layer, and
/// nothing on it reaches an HTTP response — not the id, not the region, not the
/// connector (§2, §26). The one thing that escapes upward is a telemetry label
/// (§29), which is internal by definition.
///
/// `Debug` is safe to derive: a [`DataSource`] holds a
/// [`ConnectionSelector`](fabric_connector::ConnectionSelector), never a
/// resolved credential, and `ResolvedSecret` could not print itself even if one
/// were reachable.
#[derive(Debug, Clone)]
pub struct ResolvedDataSource {
    /// Where to execute.
    pub target: ExecutionTarget,

    /// The DataSource this resolved to.
    pub data_source: Arc<DataSource>,
}

impl ResolvedDataSource {
    /// Whether the platform permits writes to this DataSource.
    ///
    /// Distinct from whether the *connector* supports mutations. Both are
    /// checked, and either saying no is a no (§28).
    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.data_source.capabilities.writable
    }

    /// A short, non-sensitive description for telemetry (§29).
    #[must_use]
    pub fn telemetry_label(&self) -> String {
        self.data_source.telemetry_label()
    }
}
