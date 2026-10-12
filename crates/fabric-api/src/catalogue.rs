//! The one decoder for the resource catalogue this host serves.
//!
//! Startup, the reader's last-known-good checks and the recovery command
//! all decode catalogue bytes here, so none of them can accept a catalogue
//! the others would refuse.

use fabric_data_api::ResourceCatalog;
use fabric_tenant_runtime::CatalogueCheck;

/// Decodes catalogue bytes as the Data API serves them.
///
/// # Errors
///
/// The decoder's message when the bytes are not a catalogue.
pub fn decode(bytes: &[u8]) -> Result<ResourceCatalog, String> {
    serde_json::from_slice(bytes).map_err(|error| error.to_string())
}

/// [`decode`] as the runtime plane's [`CatalogueCheck`], handed to the
/// reader and the recovery command.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostCatalogue;

impl CatalogueCheck for HostCatalogue {
    fn check(&self, catalogue: &[u8]) -> Result<(), String> {
        decode(catalogue).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_example_catalogue_passes_and_other_json_does_not() {
        let example = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/catalog.json"
        ))
        .unwrap();
        assert!(HostCatalogue.check(&example).is_ok());
        assert!(HostCatalogue.check(b"[]").is_err());
        assert!(HostCatalogue.check(b"{\"articles\": 1}").is_err());
        assert!(HostCatalogue.check(b"{ torn").is_err());
    }
}
