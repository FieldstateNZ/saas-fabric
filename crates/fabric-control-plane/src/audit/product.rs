//! Audit records for the product catalogue slice: creating a client,
//! changing its product configuration, and changing the catalogue itself.
//!
//! Split from `audit`'s own file for the reason `logging::integration` is
//! split from `logging`: one cohesive group of events added together, kept
//! apart from the identity and secrets events that were there first, rather
//! than grown onto the end of a file already at its own size limit.

use fabric_client_model::{ClientId, ClientRevision};
use fabric_core::{event_id, EventType};

use crate::{Operator, DOMAIN_ID};

/// A client was created.
///
/// Its own record rather than a reuse of
/// [`identity_updated`](super::identity_updated)'s shape, because creation
/// writes one document that establishes both identity and product
/// configuration in a single commit — there is no earlier "client exists but
/// has no identity yet" state for an identity-only event to describe.
pub(crate) fn client_created(operator: &Operator, client: &ClientId, revision: &ClientRevision) {
    tracing::info!(
        event = "control_plane.audit.client_created",
        event_id = event_id(DOMAIN_ID, EventType::Success, 10),
        operation = "create_client",
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        "operator created a client"
    );
}

/// A client's product configuration — legal name, region, timezone, custom
/// fields or application entitlements — was replaced.
pub(crate) fn product_updated(operator: &Operator, client: &ClientId, revision: &ClientRevision) {
    tracing::info!(
        event = "control_plane.audit.product_updated",
        event_id = event_id(DOMAIN_ID, EventType::Success, 11),
        operation = "update_client_product",
        requested_by = operator.subject(),
        client_id = %client,
        revision = %revision,
        "operator changed a client's product configuration"
    );
}

/// The product catalogue was changed by one operator command.
///
/// # Why there is no `client_id` here
///
/// The catalogue is not a client's document — it is the one desired-state
/// document with no client to name, so [`identity_updated`](super::identity_updated)
/// and [`client_secret`](super::client_secret)'s `client_id` field is
/// replaced by a fixed `resource = "catalogue"` instead.
///
/// # Why `action` and `entry` are read back rather than recomputed
///
/// [`Catalogue::apply`](fabric_client_model::catalogue::Catalogue::apply)
/// already decides exactly this pair for every command — `"Application
/// created"`, the application's id — and appends it to the catalogue's own
/// activity feed, which `GET /api/activity` shows an operator. Passing the
/// same values here, read back from that feed rather than matched on the
/// command a second time, is what keeps the audit trail and the console
/// unable to disagree about what just happened.
pub(crate) fn catalogue_changed(operator: &Operator, action: &str, entry: &str, revision: &ClientRevision) {
    tracing::info!(
        event = "control_plane.audit.catalogue_changed",
        event_id = event_id(DOMAIN_ID, EventType::Success, 12),
        operation = action,
        requested_by = operator.subject(),
        resource = "catalogue",
        entry,
        revision = %revision,
        "operator changed the product catalogue"
    );
}
