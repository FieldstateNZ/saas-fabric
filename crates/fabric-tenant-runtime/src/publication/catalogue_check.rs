//! The host's own check that catalogue bytes can be served.

/// Decides whether catalogue bytes are a catalogue the host can serve.
///
/// The catalogue's type lives in `fabric-data-api`, which depends on this
/// crate, so this crate cannot decode it. The host passes its own decoder
/// in, the same one it starts from, so a generation is never accepted, kept
/// or reported valid on a weaker reading of the catalogue than the one that
/// will serve it.
pub trait CatalogueCheck: Send + Sync {
    /// `Ok` when the bytes decode as the host's catalogue.
    ///
    /// # Errors
    ///
    /// Why they do not, for logs and the recovery command.
    fn check(&self, catalogue: &[u8]) -> Result<(), String>;
}
