//! Startup negotiation and wiring for an NDC connector.

use std::sync::Arc;

use fabric_connector::SecretResolver;

use crate::client::NdcHttpClient;
use crate::translate::to_capabilities;
use crate::wire::{NdcCapabilitiesResponse, NdcSchemaResponse};
use crate::{logging, NdcConnector, NdcConnectorConfig, SchemaIndex, NDC_VERSION};

/// Negotiates with a connector and builds it.
///
/// Performs the two startup calls — `GET /capabilities` and `GET /schema` — and
/// caches both. Nothing here happens again on the request path: §6's principle
/// that discovery belongs before request handling applies to connectors just as
/// it does to tenant bindings.
///
/// # Errors
///
/// Returns a message if the configuration is invalid, the connector is
/// unreachable, or it implements an incompatible specification version.
/// Failing at startup is deliberate: a connector that cannot be negotiated
/// cannot serve any tenant bound to it, and finding that out at boot beats
/// finding out under load.
pub async fn build_ndc_connector(
    config: NdcConnectorConfig,
    secrets: Option<Arc<dyn SecretResolver>>,
) -> Result<Arc<NdcConnector>, String> {
    config.validate()?;

    let client = NdcHttpClient::new(&config)?;

    let capabilities: NdcCapabilitiesResponse = client
        .get("/capabilities")
        .await
        .map_err(|error| format!("connector {}: could not read capabilities: {error}", config.id))?;

    match check_version(config.id.as_str(), &capabilities.version)? {
        VersionOutcome::Matched => {}
        VersionOutcome::PatchMismatch { connector_version } => {
            logging::version_patch_mismatch(config.id.as_str(), &connector_version, NDC_VERSION);
        }
    }

    let schema: NdcSchemaResponse = client
        .get("/schema")
        .await
        .map_err(|error| format!("connector {}: could not read schema: {error}", config.id))?;

    let index = SchemaIndex::build(&schema);
    let neutral_capabilities = to_capabilities(&capabilities, &index, &config);

    logging::connector_ready(
        config.id.as_str(),
        &capabilities.version,
        index.neutral().collection_names().count(),
        neutral_capabilities.mutations,
    );

    Ok(Arc::new(NdcConnector::new(
        config,
        client,
        neutral_capabilities,
        index,
        secrets,
    )))
}

/// What checking a connector's version against ours found.
///
/// A plain `Result<(), String>` would collapse "matched exactly" and "matched
/// well enough to warn about" into the same success value, which leaves the
/// caller unable to tell them apart without re-deriving the comparison
/// itself. Naming both outcomes keeps the warning's trigger — a version that
/// is *compatible but not identical* — a fact the type carries, not a side
/// effect buried inside `check_version`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum VersionOutcome {
    /// The connector's version matches [`NDC_VERSION`] exactly.
    Matched,
    /// Same major and minor, but a different patch. Accepted — the wire
    /// format is stable within a minor version — but worth a warning so the
    /// drift is visible to an operator.
    PatchMismatch {
        /// The version the connector reported.
        connector_version: String,
    },
}

/// Checks the connector's specification version against ours.
///
/// A differing **patch** version is tolerated: the wire format is stable
/// within a minor version, and refusing to start over a patch bump would make
/// every connector upgrade a coordinated release. The caller is still told,
/// via [`VersionOutcome::PatchMismatch`], so it can log the drift.
///
/// A differing **major or minor** version is fatal, in either direction —
/// older or newer. Our wire types are hand-written against one version of the
/// specification, so a connector speaking a different one may serialise
/// fields we do not read or expect fields we do not send — and the resulting
/// failure would appear as malformed responses under load rather than as a
/// clear error at boot.
///
/// A version string that cannot be parsed as `major.minor[.patch]` is treated
/// as its own opaque value rather than defaulted to anything — see
/// [`minor_version`] — so it can only ever compare unequal to ours and is
/// therefore rejected, never silently accepted.
///
/// # Errors
///
/// A message naming both versions, for an incompatible major/minor.
fn check_version(connector: &str, connector_version: &str) -> Result<VersionOutcome, String> {
    let ours = minor_version(NDC_VERSION);
    let theirs = minor_version(connector_version);

    if ours != theirs {
        return Err(format!(
            "connector {connector} implements NDC {connector_version}, but this client implements \
             {NDC_VERSION}; major/minor versions must match"
        ));
    }

    if connector_version == NDC_VERSION {
        return Ok(VersionOutcome::Matched);
    }

    Ok(VersionOutcome::PatchMismatch {
        connector_version: connector_version.to_owned(),
    })
}

/// Extracts `major.minor` from a version string.
///
/// A version we cannot parse compares as itself, so an unparseable version on
/// either side is a mismatch rather than a silent pass.
fn minor_version(version: &str) -> String {
    let mut parts = version.split('.');

    match (parts.next(), parts.next()) {
        (Some(major), Some(minor)) => format!("{major}.{minor}"),
        _ => version.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Supported version (exact match) -> accepted --------------------

    #[test]
    fn an_exact_version_match_is_accepted() {
        assert_eq!(
            check_version("postgres", NDC_VERSION),
            Ok(VersionOutcome::Matched)
        );
    }

    // -- Same major.minor, different patch -> accepted with a warning ---

    #[test]
    fn a_patch_difference_is_accepted_as_a_reportable_mismatch() {
        // `PatchMismatch` carrying the connector's version is what
        // `build_ndc_connector` matches on to log `version_patch_mismatch` —
        // asserting the variant here is what proves the warning path is
        // actually reached, independent of asserting on a tracing sink.
        assert_eq!(
            check_version("postgres", "0.2.9"),
            Ok(VersionOutcome::PatchMismatch {
                connector_version: "0.2.9".to_owned()
            })
        );
    }

    #[test]
    fn a_patch_difference_the_other_direction_is_also_accepted() {
        // 0.2.13 is our pinned patch; a connector ahead of it on the same
        // minor is exactly as compatible as one behind it.
        assert!(matches!(
            check_version("postgres", "0.2.99"),
            Ok(VersionOutcome::PatchMismatch { .. })
        ));
    }

    // -- Newer minor (0.3.x) -> rejected at startup with a clear error ---

    #[test]
    fn a_newer_minor_version_is_rejected() {
        let error = check_version("postgres", "0.3.0").unwrap_err();

        assert!(error.contains("major/minor versions must match"));
        assert!(error.contains("0.3.0"));
    }

    // -- Older minor (0.1.x) -> rejected at startup with a clear error ---

    #[test]
    fn an_older_minor_version_is_rejected() {
        let error = check_version("postgres", "0.1.9").unwrap_err();

        assert!(error.contains("major/minor versions must match"));
        assert!(error.contains("0.1.9"));
    }

    // -- Newer major (1.x) -> rejected -----------------------------------

    #[test]
    fn a_newer_major_version_is_rejected() {
        assert!(check_version("postgres", "1.0.0").is_err());
    }

    #[test]
    fn a_newer_major_version_is_rejected_even_if_the_minor_number_coincides() {
        // Major takes precedence: "1.2" is not "compatible enough" just
        // because its minor digit happens to equal ours.
        assert!(check_version("postgres", "1.2.13").is_err());
    }

    // -- Malformed / unparseable version -> rejected, never silently -----
    // -- accepted --------------------------------------------------------

    #[test]
    fn an_unparseable_version_does_not_pass_silently() {
        assert!(check_version("postgres", "experimental").is_err());
    }

    #[test]
    fn an_empty_version_does_not_pass_silently() {
        assert!(check_version("postgres", "").is_err());
    }

    #[test]
    fn a_version_with_a_non_numeric_minor_does_not_pass_silently() {
        assert!(check_version("postgres", "0.x.13").is_err());
    }

    #[test]
    fn a_version_with_only_a_major_component_does_not_pass_silently() {
        assert!(check_version("postgres", "2").is_err());
    }

    // -- minor_version helper ---------------------------------------------

    #[test]
    fn extracts_major_and_minor() {
        assert_eq!(minor_version("0.2.13"), "0.2");
        assert_eq!(minor_version("0.2"), "0.2");
        assert_eq!(minor_version("nonsense"), "nonsense");
    }
}
