//! Reading what Keycloak currently holds.

mod clients;

use std::collections::BTreeSet;

use fabric_client_model::{RealmName, RoleName};
use fabric_reconciliation::{ObservedRealm, ProviderError};

use crate::admin::KeycloakAdmin;
use crate::logging;
use crate::wire::{RealmRepresentation, RoleRepresentation};

/// The most roles this adapter will read in one request.
///
/// Not a silent cap: a realm returning exactly this many is reported as a
/// failure rather than reconciled against a truncated list. Quietly working
/// from a partial set would leave a client permanently reporting changes it
/// had already made, and no log line would say why.
const ROLE_PAGE: usize = 2000;

/// Reads a realm's state, or reports that it does not exist.
pub(super) async fn realm(
    admin: &KeycloakAdmin,
    realm: &RealmName,
) -> Result<Option<ObservedRealm>, ProviderError> {
    let representation: Option<RealmRepresentation> = admin
        .get_optional("reading the realm", admin.paths().realm(realm))
        .await?;

    let Some(representation) = representation else {
        return Ok(None);
    };

    Ok(Some(ObservedRealm {
        display_name: representation.display_name.unwrap_or_default(),
        roles: roles(admin, realm).await?,
        clients: clients::read(admin, realm).await?,
    }))
}

/// Reads a realm's roles.
async fn roles(admin: &KeycloakAdmin, realm: &RealmName) -> Result<BTreeSet<RoleName>, ProviderError> {
    let representations: Vec<RoleRepresentation> = admin
        .get("reading realm roles", admin.paths().roles_page(realm, ROLE_PAGE))
        .await?;

    if representations.len() >= ROLE_PAGE {
        return Err(ProviderError::Rejected {
            detail: format!("the realm has more than {ROLE_PAGE} roles, which this adapter cannot read"),
        });
    }

    Ok(representations
        .into_iter()
        .filter_map(|role| parse_role(&role.name))
        .collect())
}

/// Parses a role name Keycloak reported, ignoring ones this model cannot hold.
///
/// # Why skipping is safe here and would not be elsewhere
///
/// A *declared* role always parses — it came from a document this platform
/// validated. So a name that fails to parse is by definition one SaaS Fabric
/// did not declare, and one it will therefore never look for. Dropping it
/// changes no decision the reconciler makes; keeping it would mean widening
/// [`RoleName`] to hold values the platform refuses to write.
fn parse_role(name: &str) -> Option<RoleName> {
    let Ok(role) = RoleName::try_new(name) else {
        logging::unmodellable_role_ignored();
        return None;
    };

    Some(role)
}
