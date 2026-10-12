//! The one decoder for the resource catalogue a host serves.
//!
//! The runtime installs what this decodes with each generation, and the
//! reader's last-known-good checks and the recovery command judge catalogue
//! bytes by it, so none of them can accept a catalogue the others would
//! refuse (G8).

use fabric_tenant_runtime::{CatalogueDecoder, DecodedCatalogue};

use crate::ResourceCatalog;

/// Decodes catalogue bytes as the Data API serves them.
///
/// # Errors
///
/// The decoder's message when the bytes are not a catalogue, or when the
/// catalogue is empty, since a view with no resource could serve nothing.
pub fn decode_catalogue(bytes: &[u8]) -> Result<ResourceCatalog, String> {
    let catalogue: ResourceCatalog = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if catalogue.is_empty() {
        return Err("the resource catalogue is empty, so no resource can be served".to_owned());
    }
    Ok(catalogue)
}

/// [`decode_catalogue`] as the runtime plane's [`CatalogueDecoder`], handed
/// to the reader, its last-known-good copy and the recovery command.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceCatalogDecoder;

impl CatalogueDecoder for ResourceCatalogDecoder {
    fn decode(&self, catalogue: &[u8]) -> Result<DecodedCatalogue, String> {
        decode_catalogue(catalogue).map(DecodedCatalogue::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_example_catalogue_decodes_as_a_resource_catalog_and_other_json_does_not() {
        let example = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/catalog.json"
        ))
        .unwrap();
        let decoded = ResourceCatalogDecoder.decode(&example).unwrap();
        assert!(decoded
            .get::<ResourceCatalog>()
            .is_some_and(|catalogue| !catalogue.is_empty()));
        assert!(ResourceCatalogDecoder.decode(b"[]").is_err());
        assert!(ResourceCatalogDecoder.decode(b"{\"articles\": 1}").is_err());
        assert!(ResourceCatalogDecoder.decode(b"{ torn").is_err());
    }

    #[test]
    fn an_empty_catalogue_is_refused() {
        let refused = ResourceCatalogDecoder.decode(b"{}").unwrap_err();

        assert!(refused.contains("empty"), "{refused}");
    }
}
