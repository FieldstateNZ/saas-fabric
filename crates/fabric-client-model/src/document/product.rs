//! Creation and product edits preserve every unrelated client document section.
//!
//! Over the 120-line advisory threshold. The reason is that this is one impl
//! block — `ClientDocument`'s three product operations — together with the
//! `NewDocument`/`Metadata`/`Spec` wire shape that only `create` needs to
//! write a brand-new `v2` document from nothing. That shape and the one
//! function that produces it are inseparable: splitting the three structs
//! into their own file would move the same fields one file over without
//! making either easier to read.
use super::{render, schema};
use crate::catalogue::{ClientProduct, ClientProductRequest};
use crate::{ClientDocument, ClientId, DesiredStateError, IdentityConfiguration, RealmName, RoleName};
use serde::Serialize;
use serde_norway::Value;
impl ClientDocument {
    /// Reads the optional product section of an existing client.
    /// # Errors
    /// Rejects a malformed stored product section rather than hiding it.
    pub fn product(&self) -> Result<ClientProduct, DesiredStateError> {
        self.raw()
            .get("spec")
            .and_then(|spec| spec.get("product"))
            .map_or_else(
                || Ok(ClientProduct::default()),
                |value| serde_norway::from_value(value.clone()).map_err(|error| malformed(&error)),
            )
    }
    /// Creates a v2 client document with required identity roles.
    /// # Errors
    /// Returns validation errors before any repository write.
    pub fn create(
        id: &ClientId,
        request: &ClientProductRequest,
        product: &ClientProduct,
    ) -> Result<Self, DesiredStateError> {
        let roles = crate::required_roles::REQUIRED_ROLES
            .iter()
            .map(|role| RoleName::try_new(*role))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| identifier(&error))?;
        let identity = IdentityConfiguration {
            realm: RealmName::try_new(id.as_str()).map_err(|error| identifier(&error))?,
            roles,
            clients: vec![],
        };
        let raw = NewDocument {
            api_version: schema::API_VERSION_V2,
            kind: schema::KIND,
            metadata: Metadata { name: id },
            spec: Spec {
                display_name: &request.display_name,
                hosts: &request.hosts,
                identity,
                product,
            },
        };
        let document = Self::parse(&serde_norway::to_string(&raw).map_err(|error| malformed(&error))?)?;
        document.with_application_identity()
    }
    /// Changes core product fields without round-tripping unmodeled sections.
    /// # Errors
    /// Rejects invalid core fields and malformed documents.
    pub fn with_product(
        &self,
        request: &ClientProductRequest,
        product: &ClientProduct,
    ) -> Result<Self, DesiredStateError> {
        let previous = self.product()?;
        for assignment in &product.applications {
            let owned = previous
                .applications
                .iter()
                .any(|a| a.application_id == assignment.application_id);
            if !owned
                && self
                    .client()
                    .identity
                    .clients
                    .iter()
                    .any(|c| c.id.as_str() == assignment.application_id.as_str())
            {
                return Err(DesiredStateError::InvalidField {
                    field: "applications",
                    detail: "An existing identity client already uses this application identifier".into(),
                });
            }
        }
        let migrated = self.with_identity(self.client().identity.clone())?;
        let mut raw = migrated.raw().clone();
        let spec = raw
            .get_mut("spec")
            .and_then(Value::as_mapping_mut)
            .ok_or(DesiredStateError::MissingField { field: "spec" })?;
        spec.insert(
            Value::String("displayName".into()),
            Value::String(request.display_name.clone()),
        );
        spec.insert(
            Value::String("hosts".into()),
            serde_norway::to_value(&request.hosts).map_err(|error| malformed(&error))?,
        );
        spec.insert(
            Value::String("product".into()),
            serde_norway::to_value(product).map_err(|error| malformed(&error))?,
        );
        Self::parse(&render::render(&raw)?)?.with_application_identity()
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NewDocument<'a> {
    api_version: &'a str,
    kind: &'a str,
    metadata: Metadata<'a>,
    spec: Spec<'a>,
}
#[derive(Serialize)]
struct Metadata<'a> {
    name: &'a ClientId,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Spec<'a> {
    display_name: &'a str,
    hosts: &'a [crate::Host],
    identity: IdentityConfiguration,
    product: &'a ClientProduct,
}
fn malformed(error: &serde_norway::Error) -> DesiredStateError {
    DesiredStateError::Malformed {
        detail: error.to_string(),
    }
}

fn identifier(error: &fabric_core::IdentifierError) -> DesiredStateError {
    DesiredStateError::InvalidField {
        field: "identity",
        detail: error.to_string(),
    }
}
