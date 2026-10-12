//! The one decoder for the resource catalogue a host serves.
//!
//! The runtime installs what this decodes with each generation, and the
//! reader's last-known-good checks and the recovery command judge catalogue
//! bytes by it, so none of them can accept a catalogue the others would
//! refuse (G8).

use fabric_tenant_runtime::{CatalogueDecoder, DecodedCatalogue};
use serde_json::error::Category;

use crate::ResourceCatalog;

/// Decodes catalogue bytes as the Data API serves them.
///
/// # Errors
///
/// Why the bytes are not a catalogue, as a category and a line and column
/// (never the published content), or that the catalogue is empty, since a view with no
/// resource could serve nothing.
pub fn decode_catalogue(bytes: &[u8]) -> Result<ResourceCatalog, String> {
    let catalogue: ResourceCatalog = serde_json::from_slice(bytes).map_err(|error| describe(&error))?;
    if catalogue.is_empty() {
        return Err("the resource catalogue is empty, so no resource can be served".to_owned());
    }
    Ok(catalogue)
}

/// What a decoding failure may say: its category and where it is, never the
/// decoder's own message. That message quotes the offending value or key,
/// and this text reaches the readiness detail, the reader's logs and the
/// recovery command, none of which may carry arbitrary published content.
fn describe(error: &serde_json::Error) -> String {
    let what = match error.classify() {
        Category::Io => "could not be read",
        Category::Syntax => "is not valid JSON",
        Category::Data => "does not match the catalogue shape",
        Category::Eof => "ends before its JSON does",
    };
    format!(
        "the catalogue {what} at line {} column {}",
        error.line(),
        error.column()
    )
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

    /// Review of #170: the decoder's own message quoted a malformed value,
    /// which reached `last_hold` and the reader's log.
    #[test]
    fn a_refusal_names_a_category_and_a_location_and_never_the_offending_value_or_key() {
        let sentinel = "SYNTHETIC_SECRET_170_NOT_REAL";
        let cases = [
            format!(
                r#"{{"articles": {{"data_source": "primary", "collection": "articles", "operations": ["{sentinel}"]}}}}"#
            ),
            format!(
                r#"{{"articles": {{"data_source": "primary", "collection": "articles", "{sentinel}": 1}}}}"#
            ),
            format!(r#"{{"articles": {{"data_source": "{sentinel}"#),
            format!(r#"{{"{sentinel}" "#),
        ];
        for case in cases {
            let refused = ResourceCatalogDecoder.decode(case.as_bytes()).unwrap_err();

            assert!(!refused.contains(sentinel), "{refused}");
            assert!(refused.starts_with("the catalogue "), "{refused}");
            assert!(refused.contains(" at line 1 column "), "{refused}");
        }
    }

    #[test]
    fn an_empty_catalogue_is_refused() {
        let refused = ResourceCatalogDecoder.decode(b"{}").unwrap_err();

        assert!(refused.contains("empty"), "{refused}");
    }
}
