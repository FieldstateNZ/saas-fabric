//! What Fabric can say about the issuer registry being applied.

/// What Fabric knows about the registry being applied. Fabric publishes the
/// registry; no runtime reads it, and edge application is platform
/// composition's, so the only state Fabric can report is that it observes
/// none. Until an applied generation is evidenced, an excluded tenant may
/// still be served by trust configured elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryApplication {
    /// No edge or runtime application of any registry generation is
    /// observed.
    Unobserved,
}
