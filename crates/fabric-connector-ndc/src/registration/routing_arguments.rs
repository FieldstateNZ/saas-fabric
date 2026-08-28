//! Checking a connector can carry the routing this configuration depends on.

use crate::wire::{NdcRequestLevelArguments, NdcSchemaResponse};
use crate::NdcConnectorConfig;

/// Refuses a connector whose declared request arguments cannot route tenants.
///
/// # What goes wrong without this
///
/// Nothing visible, which is the whole difficulty. `schema/arguments.md` says
/// nothing about what a connector must do with a request-level argument it
/// never declared, so the behaviour is connector-defined — and the one
/// implementation that can be read chooses to ignore it. A statically
/// configured `ndc-postgres` declares no request arguments at all, and its pool
/// acquisition returns the single static pool without consulting them: every
/// tenant bound to that connector runs against the same database, and every
/// request answers `200`. A name-mode deployment with a fallback pool looks
/// only for `connection_name`, so a tenant routed by connection *string* lands
/// in the fallback database instead. Both are cross-tenant reads that no
/// response distinguishes from a correct one.
///
/// # Why the configuration is what gets checked
///
/// A connector is negotiated long before any tenant is resolved, so "will a
/// tenant be name-routed through this connector?" has no answer here. What does
/// have an answer is what the configuration asks for: naming
/// `connection_name_argument` is the operator stating this connector will route
/// by name, and naming `connection_string_argument` is the same statement about
/// secret-backed connections. Either name is therefore a requirement the
/// connector must meet, and §6's discovery-before-serving principle — which
/// [`build_ndc_connector`](super::build_ndc_connector) already states — says it
/// meets it at boot or the connector is not built.
///
/// Both request kinds are required, not either. A connector accepting
/// `connection_name` on queries but not on mutations would read the right
/// tenant's rows and write into somebody else's.
///
/// # Errors
///
/// A message naming the setting, the argument, and which request kinds are
/// missing it.
pub(super) fn check_routing_arguments(
    config: &NdcConnectorConfig,
    schema: &NdcSchemaResponse,
) -> Result<(), String> {
    for (setting, argument) in configured_arguments(config) {
        let Some(argument) = argument else { continue };

        check_one(config, schema.request_arguments.as_ref(), setting, argument)?;
    }

    Ok(())
}

/// The routing arguments this configuration expects a connector to accept,
/// paired with the setting that named each one.
///
/// A free function rather than a method on [`NdcConnectorConfig`] because it
/// exists solely for the check below, and the pairing it produces is that
/// check's concern rather than the configuration's — the setting name is
/// carried only so the error can point at the line an operator has to change.
/// "The connector does not declare `connection_name`" leaves them hunting;
/// naming `connection_name_argument` does not.
fn configured_arguments(config: &NdcConnectorConfig) -> [(&'static str, Option<&str>); 2] {
    [
        (
            "connection_name_argument",
            config.connection_name_argument.as_deref(),
        ),
        (
            "connection_string_argument",
            config.connection_string_argument.as_deref(),
        ),
    ]
}

/// Checks one configured argument against what the connector declared.
fn check_one(
    config: &NdcConnectorConfig,
    declared: Option<&NdcRequestLevelArguments>,
    setting: &str,
    argument: &str,
) -> Result<(), String> {
    let Some(declared) = declared else {
        return Err(format!(
            "connector {}: {setting} names `{argument}`, but the connector's schema declares no \
             request-level arguments at all, so it would ignore the routing on every request and \
             serve every tenant from whichever connection it was started with",
            config.id
        ));
    };

    let missing = missing_request_kinds(declared, argument);

    if missing.is_empty() {
        return Ok(());
    }

    Err(format!(
        "connector {}: {setting} names `{argument}`, but the connector's schema does not declare \
         it as a request-level argument for {}; an argument a connector never declared is not \
         promised to have any effect, so tenants would be routed to the wrong connection",
        config.id,
        missing.join(" or "),
    ))
}

/// Which request kinds the connector failed to declare the argument for.
fn missing_request_kinds(declared: &NdcRequestLevelArguments, argument: &str) -> Vec<&'static str> {
    [
        ("queries", declared.declares_for_query(argument)),
        ("mutations", declared.declares_for_mutation(argument)),
    ]
    .into_iter()
    .filter_map(|(kind, declared)| (!declared).then_some(kind))
    .collect()
}
