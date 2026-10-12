//! The two decisions a restore makes about held metadata and offered
//! content: which manifest it may write, and which resource rules the
//! offered document breaks, alone or against a usable held peer.

use super::held_parse::{read_manifest, ManifestReading, Parsed};
use super::held_parts::{parsed_data_sources, parsed_tenants};
use super::semantics::{dangling, data_source_problems, registry_problems, tenant_problems};
use super::RestoreRefusal;
use crate::{DocumentKind, DocumentManifest, DocumentRevision, PublicationRecord, TargetObservation};

/// The manifest to write: one at the recorded revision when none is held or
/// the held one is not a JSON object, none when a valid one states that
/// revision, and a refusal for any other readable metadata.
pub(super) fn manifest_plan(
    held: Option<&[u8]>,
    document: DocumentKind,
    recorded: DocumentRevision,
) -> Result<Option<DocumentManifest>, RestoreRefusal> {
    match read_manifest(held, document) {
        ManifestReading::Missing | ManifestReading::Malformed(_) => {
            Ok(Some(DocumentManifest::new(document, recorded)))
        }
        ManifestReading::Valid(manifest) if manifest.revision() == recorded => Ok(None),
        ManifestReading::Valid(manifest) => Err(RestoreRefusal::ManifestDisagrees {
            document,
            manifest: manifest.revision(),
            recorded,
        }),
        ManifestReading::Conflicting(detail) => Err(RestoreRefusal::ManifestConflicts { document, detail }),
        ManifestReading::Newer(manifest) => Err(RestoreRefusal::ManifestConflicts {
            document,
            detail: format!(
                "stamped by publisher contract {}, newer than this one",
                manifest.contract_version()
            ),
        }),
    }
}

/// Every resource rule the offered document breaks. A peer that does not
/// parse is not checked against; the plan reports it instead.
pub(super) fn offered_problems(
    parsed: &Parsed,
    observation: &TargetObservation,
    record: &PublicationRecord,
) -> Vec<String> {
    match parsed {
        Parsed::Tenants(tenants) => {
            let mut problems = tenant_problems(tenants);
            if let Some(data_sources) = parsed_data_sources(observation) {
                problems.extend(dangling(tenants, &data_sources));
            }
            if let Some(recorded) = record.registry() {
                if registry_problems(observation.registry.as_deref(), recorded, None).is_empty() {
                    problems.extend(registry_problems(
                        observation.registry.as_deref(),
                        recorded,
                        Some(tenants),
                    ));
                }
            }
            problems
        }
        Parsed::DataSources(data_sources) => {
            let mut problems = data_source_problems(data_sources);
            if let Some(tenants) = parsed_tenants(observation) {
                problems.extend(dangling(&tenants, data_sources));
            }
            problems
        }
        Parsed::Catalog => Vec::new(),
    }
}
