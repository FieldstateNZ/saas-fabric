//! The host's decoder for catalogue bytes, and what it decodes them into.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// Decodes catalogue bytes into the catalogue the host serves.
///
/// The catalogue's type lives in `fabric-data-api`, which depends on this
/// crate, so this crate cannot name it. The host passes its own decoder in,
/// and the refresher installs what it decodes in the same view as the tenants
/// and data sources of that generation (G8). The same decoder judges the
/// last-known-good copy and the recovery command, so no generation is
/// accepted, kept or reported valid on a weaker reading of the catalogue than
/// the one that serves it.
pub trait CatalogueDecoder: Send + Sync {
    /// The catalogue these bytes hold.
    ///
    /// # Errors
    ///
    /// Why they are not a catalogue the host can serve, for logs, readiness
    /// and the recovery command.
    fn decode(&self, catalogue: &[u8]) -> Result<DecodedCatalogue, String>;
}

/// A catalogue the host decoded, opaque to this crate.
///
/// The host reads it back as its own type with [`Self::get`]. Cloning shares
/// the one decoded value.
#[derive(Clone)]
pub struct DecodedCatalogue(Arc<dyn Any + Send + Sync>);

impl DecodedCatalogue {
    /// Wraps the host's decoded catalogue.
    #[must_use]
    pub fn new<T: Any + Send + Sync>(catalogue: T) -> Self {
        Self(Arc::new(catalogue))
    }

    /// The decoded catalogue, when it is a `T`.
    #[must_use]
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.0.downcast_ref::<T>()
    }
}

impl fmt::Debug for DecodedCatalogue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DecodedCatalogue")
    }
}
