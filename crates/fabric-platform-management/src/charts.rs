//! The port through which chart versions are discovered.

use crate::{RegistryError, Version};

/// What a chart repository knows about one chart.
///
/// # Deliberately not the registry port
///
/// A chart repository answers a different question with a weaker answer. There
/// is no digest to pin and no provenance to check — an index lists versions,
/// and that is all it lists. Reusing [`Registry`](crate::Registry) would have
/// meant a `resolve` that returns a digest nobody has and a provenance nobody
/// checked, which is the shape of an abstraction that has been stretched over
/// something it does not fit.
#[async_trait::async_trait]
pub trait ChartIndex: Send + Sync {
    /// Every version of a chart the repository publishes.
    ///
    /// Unordered, and not filtered: which of them a component may move to is
    /// the caller's question, and it depends on a channel and a floor this
    /// port knows nothing about.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the repository could not be asked. An empty list
    /// means the chart is not published there, which is a different thing from
    /// not being able to find out.
    async fn versions(&self, repository: &str, chart: &str) -> Result<Vec<Version>, RegistryError>;
}
