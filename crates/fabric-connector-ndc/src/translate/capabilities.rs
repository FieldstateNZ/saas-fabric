//! What the connector told us it can do, in neutral terms.

use fabric_connector::ConnectorCapabilities;

use crate::wire::NdcCapabilitiesResponse;
use crate::{NdcConnectorConfig, SchemaIndex};

/// Builds the neutral capability set for a connector.
///
/// Three sources combine here:
///
/// - **The protocol.** Filtering, ordering, and paging are core NDC, required
///   of every conforming connector, so they are unconditionally true. They are
///   not negotiated and there is no capability key for them.
/// - **The capabilities response.** Optional extras such as transactional
///   mutations, signalled by key presence.
/// - **Our own configuration.** Writes require a procedure mapping
///   ([`CollectionProcedures`](crate::CollectionProcedures)), so a connector
///   that could accept writes still reports `mutations: false` until one is
///   configured. That is the fail-closed direction.
pub(crate) fn to_capabilities(
    capabilities: &NdcCapabilitiesResponse,
    index: &SchemaIndex,
    config: &NdcConnectorConfig,
) -> ConnectorCapabilities {
    ConnectorCapabilities {
        filtering: true,
        ordering: true,
        paging: true,
        // Unconditional, unlike everything negotiated below it. `is_null` is
        // a core NDC unary operator rather than a declared capability --
        // `NdcCapabilitiesResponse` has no key for it, and `to_expression`
        // emits it without consulting the schema -- so every conforming
        // connector can express a null test.
        null_checks: true,
        mutations: config.has_writes(),
        transactional_mutations: capabilities.supports_transactional_mutations(),
        // The Data API does not issue aggregate queries, so even where the
        // connector could count, the platform does not ask it to.
        total_count: false,
        comparisons: index.supported_operators().clone(),
    }
}
