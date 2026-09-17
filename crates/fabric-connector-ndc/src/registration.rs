//! Startup negotiation and wiring for an NDC connector.

mod key_arguments;
#[cfg(test)]
mod key_arguments_tests;
mod procedure_arguments;
#[cfg(test)]
mod procedure_arguments_tests;
mod required_arguments;
#[cfg(test)]
mod required_arguments_tests;
mod routing_arguments;
#[cfg(test)]
mod routing_arguments_tests;
mod version;
#[cfg(test)]
mod version_tests;

use std::sync::Arc;

use fabric_connector::SecretResolver;

use crate::client::NdcHttpClient;
use crate::registration::key_arguments::check_key_arguments;
use crate::registration::procedure_arguments::check_procedure_arguments;
use crate::registration::required_arguments::check_required_arguments;
use crate::registration::routing_arguments::check_routing_arguments;
use crate::registration::version::{check_version, VersionOutcome};
use crate::translate::to_capabilities;
use crate::wire::{NdcCapabilitiesResponse, NdcSchemaResponse};
use crate::{logging, NdcConnector, NdcConnectorConfig, SchemaIndex, NDC_MINIMUM_VERSION};

/// Negotiates with a connector and builds it.
///
/// Performs the two startup calls — `GET /capabilities` and `GET /schema` —
/// checks what came back, and caches both. Nothing here happens again on the
/// request path: §6's principle that discovery belongs before request handling
/// applies to connectors just as it does to tenant bindings.
///
/// Six things are checked, in the order the answers arrive:
///
/// 1. **The specification version**, against the floor this client requires —
///    see `version::check_version`.
/// 2. **The declared request-level arguments**, against the routing this
///    configuration depends on — see
///    `routing_arguments::check_routing_arguments`. This is the check that
///    stops a connector which would silently serve every tenant the same
///    database.
/// 3. **The schema itself**, indexed so the request path never has to ask
///    again.
/// 4. **The write mapping**, against the procedures and arguments the schema
///    declares — see `procedure_arguments::check_procedure_arguments`. This is
///    the check that stops a tenant predicate being sent under a name the
///    procedure never declared, which is an unscoped delete.
/// 5. **Every key argument**, against the same schema — see
///    `key_arguments::check_key_arguments`. The mirror of check 4 for the
///    arguments a keyed procedure reads its key from, rather than its
///    predicate.
/// 6. **Every argument the procedure requires**, against what *this verb's*
///    translation actually sends — payload, filter and keys for an update;
///    filter and keys for a delete; payload alone for an insert, since a
///    delete's `payload_argument` (config validation refuses one) or an
///    insert's `filter_argument` is never read — see
///    `required_arguments::check_required_arguments`. This is the check that
///    would have turned issue #62's F3 into a boot failure instead of a
///    first-write failure.
///
/// Checks 2, 4 and 5 are the same idea applied to different arguments: one a
/// connector never declared is not promised to do anything, and in each case
/// the silence is what makes it dangerous.
///
/// # Errors
///
/// Returns a message if the configuration is invalid, the connector is
/// unreachable, it implements an incompatible specification version, or it
/// cannot carry this configuration's tenant routing. Failing at startup is
/// deliberate: a connector that cannot be negotiated cannot serve any tenant
/// bound to it, and finding that out at boot beats finding out under load —
/// or, worse, not finding out at all.
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
        VersionOutcome::AheadOfFloor { connector_version } => {
            logging::version_ahead_of_floor(config.id.as_str(), &connector_version, NDC_MINIMUM_VERSION);
        }
    }

    let schema: NdcSchemaResponse = client
        .get("/schema")
        .await
        .map_err(|error| format!("connector {}: could not read schema: {error}", config.id))?;

    check_routing_arguments(&config, &schema)?;

    let index = SchemaIndex::build(&schema);

    check_procedure_arguments(&config, &index)?;
    check_key_arguments(&config, &index)?;
    check_required_arguments(&config, &index)?;

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
