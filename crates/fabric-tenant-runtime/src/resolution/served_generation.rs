//! Which generation a published runtime's resolution reads, for the
//! replica to report (G7).

use fabric_core::ReaderGenerationReport;

use super::RuntimeResolver;

impl RuntimeResolver {
    /// The generation every resolution reads now, from the one view that
    /// resolution reads: one atomic load, so the number, the record digest
    /// and the origin are always the installed view's own, never a
    /// configured, mounted or about-to-be-applied generation.
    ///
    /// `None` when nothing is installed, when the installed view holds
    /// documents with no publication record, when a registry did not take
    /// every resource of the generation as offered, and always for a
    /// resolver built over two independent sources.
    #[must_use]
    pub fn served_generation(&self) -> Option<ReaderGenerationReport> {
        self.published
            .as_ref()
            .and_then(|published| published.view().served())
    }
}

#[cfg(test)]
#[path = "served_generation_tests.rs"]
mod tests;
