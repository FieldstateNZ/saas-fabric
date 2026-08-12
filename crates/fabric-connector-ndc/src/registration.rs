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

    check_version(config.id.as_str(), &capabilities.version)?;

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

/// Checks the connector's specification version against ours.
///
/// A differing **patch** version is a warning: the wire format is stable within
/// a minor version, and refusing to start over a patch bump would make every
/// connector upgrade a coordinated release.
///
/// A differing **major or minor** version is fatal. Our wire types are
/// hand-written against one version of the specification, so a connector
/// speaking a different one may serialise fields we do not read or expect
/// fields we do not send — and the resulting failure would appear as malformed
/// responses under load rather than as a clear error at boot.
fn check_version(connector: &str, connector_version: &str) -> Result<(), String> {
    let ours = minor_version(NDC_VERSION);
    let theirs = minor_version(connector_version);

    if ours != theirs {
        return Err(format!(
            "connector {connector} implements NDC {connector_version}, but this client implements \
             {NDC_VERSION}; major/minor versions must match"
        ));
    }

    if connector_version != NDC_VERSION {
        logging::version_patch_mismatch(connector, connector_version, NDC_VERSION);
    }

    Ok(())
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

    #[test]
    fn the_pinned_version_matches_itself() {
        assert!(check_version("postgres", NDC_VERSION).is_ok());
    }

    #[test]
    fn a_patch_difference_is_tolerated() {
        assert!(check_version("postgres", "0.2.9").is_ok());
    }

    #[test]
    fn a_minor_difference_is_fatal() {
        let error = check_version("postgres", "0.3.0").unwrap_err();

        assert!(error.contains("major/minor versions must match"));
    }

    #[test]
    fn a_major_difference_is_fatal() {
        assert!(check_version("postgres", "1.2.13").is_err());
    }

    #[test]
    fn an_unparseable_version_does_not_pass_silently() {
        assert!(check_version("postgres", "experimental").is_err());
    }

    #[test]
    fn extracts_major_and_minor() {
        assert_eq!(minor_version("0.2.13"), "0.2");
        assert_eq!(minor_version("0.2"), "0.2");
        assert_eq!(minor_version("nonsense"), "nonsense");
    }
}
