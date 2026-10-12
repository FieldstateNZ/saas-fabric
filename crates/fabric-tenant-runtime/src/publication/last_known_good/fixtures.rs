//! One generation's bytes, as a publisher would write them, for the tests.

use crate::publication::complete_generation::CompleteGeneration;
use crate::publication::digest::sha256_hex;
use crate::publication::parsed_generation::ParsedGeneration;
use crate::publication::READER_CONTRACT;
use crate::publication::{CatalogueDecoder, DecodedCatalogue};
use crate::testing::{data_source, tenant_binding};

/// The four files of one publication.
pub(super) struct Published {
    pub(super) record: Vec<u8>,
    pub(super) data_sources: Vec<u8>,
    pub(super) tenants: Vec<u8>,
    pub(super) catalog: Vec<u8>,
}

/// Generation `generation`: tenant `acme` bound to `data_source`, under a
/// record whose reader floor is `reader_floor`.
pub(super) fn published_with_floor(generation: u64, data_source_id: &str, reader_floor: u32) -> Published {
    let tenants =
        serde_json::to_vec_pretty(&vec![tenant_binding("acme", generation, data_source_id)]).unwrap();
    let data_sources = serde_json::to_vec_pretty(&vec![data_source(data_source_id, generation)]).unwrap();
    let catalog = format!("{{\"generation\":{generation}}}").into_bytes();
    recorded(
        generation,
        reader_floor,
        Published {
            record: Vec::new(),
            data_sources,
            tenants,
            catalog,
        },
    )
}

/// `files` under a record naming exactly their bytes: a generation whose
/// digests all match, whatever the documents say.
pub(super) fn recorded(generation: u64, reader_floor: u32, files: Published) -> Published {
    let record = serde_json::to_vec_pretty(&serde_json::json!({
        "contract_version": 3,
        "minimum_reader_contract": reader_floor,
        "minimum_publisher_contract": 3,
        "generation": generation,
        "documents": {
            "tenants": { "revision": generation, "sha256": sha256_hex(&files.tenants) },
            "data-sources": { "revision": generation, "sha256": sha256_hex(&files.data_sources) },
            "catalog": { "revision": generation, "sha256": sha256_hex(&files.catalog) },
        },
    }))
    .unwrap();
    Published { record, ..files }
}

/// [`published_with_floor`] at this reader's own contract.
pub(super) fn published(generation: u64, data_source_id: &str) -> Published {
    published_with_floor(generation, data_source_id, READER_CONTRACT)
}

/// The same generation, verified, with its catalogue.
pub(super) fn complete(generation: u64, data_source_id: &str) -> CompleteGeneration {
    let files = published(generation, data_source_id);
    CompleteGeneration::verify(
        files.record,
        files.data_sources,
        files.tenants,
        files.catalog,
        "fixture",
    )
    .unwrap()
}

/// [`complete`], decoded and validated.
pub(super) fn parsed(generation: u64, data_source_id: &str) -> ParsedGeneration {
    ParsedGeneration::validate(complete(generation, data_source_id), &Catalogue, "fixture").unwrap()
}

/// The fixtures' stand-in for the host's catalogue decoder: a JSON object
/// carrying a numeric `generation`, which every fixture catalogue is and a
/// catalogue the tests break is not.
pub(super) struct Catalogue;

impl CatalogueDecoder for Catalogue {
    fn decode(&self, catalogue: &[u8]) -> Result<DecodedCatalogue, String> {
        let value: serde_json::Value =
            serde_json::from_slice(catalogue).map_err(|error| error.to_string())?;
        value
            .get("generation")
            .and_then(serde_json::Value::as_u64)
            .map(DecodedCatalogue::new)
            .ok_or_else(|| "not a catalogue".to_owned())
    }
}
