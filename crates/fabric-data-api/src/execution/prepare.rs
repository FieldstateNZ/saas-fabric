//! Catalogue lookup, authorization, and DataSource resolution.

use std::sync::Arc;

use fabric_core::LogicalResourceName;
use fabric_identity::TenantIdentity;

use crate::execution::prepared::Prepared;
use crate::{logging, DataApiError, DataApiService, OperationKind};

impl DataApiService {
    /// Walks the chain and authorises the operation.
    ///
    /// # Why this order
    ///
    /// One rule fixes it: **nothing that describes a resource may run before
    /// authorization.** Every step below therefore carries its own reason for
    /// sitting where it does, including the ones that look self-evident. The
    /// step that carried no reason is the one that stayed on the wrong side of
    /// this line through the pass that moved `queryable_fields` behind
    /// authorization — an unjustified position is never re-read, only assumed.
    ///
    /// 1. **Catalogue.** A request for a resource that does not exist gets a
    ///    404 regardless of scopes, so an unauthorised caller can learn which
    ///    resource *names* this deployment serves.
    ///
    ///    That is deliberate, and the reasoning has to be better than "it is
    ///    the same for every tenant" — which is what it used to say, and which
    ///    is equally true of everything authorisation now protects. Being
    ///    global is not what makes something safe to disclose. What separates
    ///    them is who the fact is *for*. Resource names are the API's route
    ///    table: a client cannot call anything without knowing them, they ship
    ///    in every client library and every piece of documentation, and
    ///    treating them as secret would mean answering 403 for a typo — turning
    ///    the most common developer mistake into the least diagnosable one. A
    ///    resource's *capabilities* are the opposite: they describe what a
    ///    caller may do with something they already have access to, so
    ///    disclosing them to someone with no access tells them something they
    ///    were never meant to have. That covers `queryable_fields` and the
    ///    `operations` list alike, and both are steps below.
    ///
    ///    If the catalogue ever carries entries whose existence is itself
    ///    sensitive, this is the line to revisit, and it is a deliberate change
    ///    rather than a gap.
    /// 2. **Authorization.** As early as the rule allows: ahead of everything
    ///    that could describe the resource, and before any registry is touched,
    ///    so neither a status code nor the time taken to produce one can be read
    ///    for facts about the tenant estate. Every step below has already
    ///    established that this caller is entitled to an answer about this
    ///    resource, which is what makes those steps safe to answer honestly.
    /// 3. **Operation allowed.** Whether the catalogue exposes this verb — a
    ///    description of the resource, on exactly the footing as
    ///    `queryable_fields`. Ahead of authorization it answered "does this
    ///    resource support delete?" for a caller who was going to be refused
    ///    anyway, so 403 against 405 enumerated every entry's `operations` list
    ///    one verb at a time.
    ///
    ///    It stays a *distinct* 405 rather than folding into the 403, because a
    ///    caller who does hold the scope is entitled to know the verb is not
    ///    offered here; answering 403 would blame their token for something the
    ///    catalogue decided. Only the unauthorised case has to be
    ///    indistinguishable. It sits directly below authorization because it
    ///    needs nothing but the catalogue entry already in hand, so a verb this
    ///    deployment does not offer never reaches a registry.
    /// 4. **Resolution.** Tenant → binding → DataSource, via the runtime
    ///    resolver. Below authorization because its failures describe the estate
    ///    — an unknown tenant, an unreconciled binding — and above the next step
    ///    because that step needs its result.
    /// 5. **Write permission.** Only after we know which DataSource this is,
    ///    because read-only is a property of the DataSource and not of the
    ///    catalogue. Step 3's reasoning one layer down: an unauthorised caller
    ///    must not learn where a tenant is placed, and an authorised one must
    ///    still be told the placement refuses writes.
    /// 6. **Connector.** Looked up by the id the DataSource named. Last because
    ///    it alone can fail for reasons outside this request — a connector still
    ///    negotiating startup (§35) — which a caller should not meet until
    ///    everything about the request itself has been accepted.
    ///
    /// # What must not happen before this runs
    ///
    /// Any check that can tell a caller something about the *resource* has to
    /// wait for the [`Prepared`] this returns — above all `queryable_fields`,
    /// via `models::field_reference::parse` and `execution::row_mapping::to_row`.
    /// A 400 that fires ahead of step 2 answers "is this a real field?" for a
    /// caller who was going to be told 403, which turns two status codes into a
    /// field-name oracle. Every such check therefore takes `prepared.resource`,
    /// so it cannot be written any other way.
    ///
    /// Checks that describe nothing about the resource may run earlier, and
    /// these deliberately do. The list is exhaustive on purpose, because an
    /// unenumerated step is one nobody audits:
    ///
    /// - **Identity**, in the `TenantIdentity` extractor. It has to be first:
    ///   there is no caller to authorise until it has run.
    /// - **The request body size cap**, in `extraction::BoundedJson`, enforced
    ///   while the body is still being read so an oversized body is never
    ///   fully buffered.
    /// - **The syntactic resource-name parse**, in `handlers::parse_resource`.
    ///   It consults no catalogue; it asks only whether the path segment could
    ///   name a resource at all.
    /// - **Body-shape normalisation**, in the create and update handlers
    ///   (`to_rows`, `to_changes`). It rejects a body that is not an object or
    ///   an array of them, and never looks at a field *name*.
    ///
    /// Each states a fixed, deployment-wide rule a caller could read off the
    /// documentation, so answering it early tells them only about what they
    /// themselves put in the request.
    pub(super) fn prepare(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        operation: OperationKind,
    ) -> Result<Prepared<'_>, DataApiError> {
        let resource = self.catalog.resolve(resource_name)?;

        self.authorize(identity, operation, resource_name)?;

        if !resource.allows(operation) {
            return Err(DataApiError::OperationNotAllowed {
                resource: resource_name.to_string(),
                operation: operation.as_str(),
            });
        }

        // The tenant comes from the identity and nowhere else (§10, §11).
        let resolved = self
            .runtime
            .resolve_data_source(identity.tenant(), &resource.data_source)?;

        if operation.is_write() && !resolved.is_writable() {
            logging::write_refused_by_data_source(resource_name, &resolved.telemetry_label());

            return Err(DataApiError::ResourceIsReadOnly {
                resource: resource_name.to_string(),
            });
        }

        let connector = Arc::clone(self.connectors.get(resolved.target.connector())?);

        Ok(Prepared {
            resource,
            resolved,
            connector,
        })
    }
}
