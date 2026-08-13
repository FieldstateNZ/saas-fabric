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
    /// 1. **Catalogue.** A request for a resource that does not exist gets a
    ///    404 regardless of scopes, so an unauthorised caller can learn which
    ///    resource *names* this deployment serves.
    ///
    ///    That is deliberate, and the reasoning has to be better than "it is
    ///    the same for every tenant" — which is what it used to say, and which
    ///    is equally true of the `queryable_fields` list that authorisation
    ///    now protects. Being global is not what makes something safe to
    ///    disclose.
    ///
    ///    What separates them is who the fact is *for*. Resource names are the
    ///    API's route table: a client cannot call anything without knowing
    ///    them, they ship in every client library and every piece of
    ///    documentation, and treating them as secret would mean answering 403
    ///    for a typo — turning the most common developer mistake into the
    ///    least diagnosable one. `queryable_fields` is the opposite: it
    ///    describes what a caller may do with a resource they already have
    ///    access to, so disclosing it to someone with no access to that
    ///    resource tells them something they were never meant to have.
    ///
    ///    If the catalogue ever carries entries whose existence is itself
    ///    sensitive, this is the line to revisit, and it is a deliberate
    ///    change rather than a gap.
    /// 2. **Operation allowed.** Whether the catalogue exposes this verb.
    /// 3. **Authorization.** Before touching any registry, so an unauthorised
    ///    caller cannot use status codes or timing to learn anything about the
    ///    tenant estate.
    /// 4. **Resolution.** Tenant → binding → DataSource, via the runtime
    ///    resolver.
    /// 5. **Write permission.** Only after we know which DataSource this is,
    ///    because read-only is a property of the DataSource.
    /// 6. **Connector.** Looked up by the id the DataSource named.
    ///
    /// # What must not happen before this runs
    ///
    /// Any check that can tell a caller something about the *resource* — above
    /// all `queryable_fields`, via `models::field_reference::parse` and
    /// `execution::row_mapping::to_row` — has to wait for the `Prepared` this
    /// returns. A 400 that fires ahead of step 3 answers "is this a real
    /// field?" for a caller who was going to be told 403, which turns two
    /// status codes into a field-name oracle. Every such check therefore takes
    /// `prepared.resource`, so it cannot be written any other way.
    ///
    /// Checks that reveal nothing about the resource may run earlier, and two
    /// deliberately do: the request body size cap, enforced while the body is
    /// still being read, and the syntactic resource-name parse in the
    /// handlers. Both describe a fixed, deployment-wide rule.
    pub(super) fn prepare(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        operation: OperationKind,
    ) -> Result<Prepared<'_>, DataApiError> {
        let resource = self.catalog.resolve(resource_name)?;

        if !resource.allows(operation) {
            return Err(DataApiError::OperationNotAllowed {
                resource: resource_name.to_string(),
                operation: operation.as_str(),
            });
        }

        self.authorize(identity, operation, resource_name)?;

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

    /// Applies the authorization policy, logging a refusal.
    ///
    /// Separate from [`Self::prepare`] so the §23 boundary is visible: this
    /// function receives an operation and an identity and returns a `Result<()>`.
    /// It is handed nothing that could change the tenant, and has no way to
    /// return one.
    fn authorize(
        &self,
        identity: &TenantIdentity,
        operation: OperationKind,
        resource_name: &LogicalResourceName,
    ) -> Result<(), DataApiError> {
        if self
            .permissions
            .permits(identity, operation, resource_name.as_str())
        {
            return Ok(());
        }

        logging::operation_forbidden(resource_name.as_str(), operation.as_str(), identity.subject());

        Err(DataApiError::Forbidden {
            resource: resource_name.to_string(),
            operation: operation.as_str(),
        })
    }
}
