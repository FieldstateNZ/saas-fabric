//! Keycloak's own representations.
//!
//! # Nothing in this module is public
//!
//! These types mirror Keycloak's admin API, field for field, and they exist so
//! that the rest of the platform never has to. Every one is `pub(crate)`, and
//! `scripts/check_architecture.py` fails the build if the names appear outside
//! this crate — the same containment the runtime plane applies to NDC's wire
//! types (ADR 0001).
//!
//! # Deliberately partial
//!
//! A real `RealmRepresentation` has well over a hundred fields. These carry
//! only what reconciliation reads or writes, and every write type omits the
//! rest so that Keycloak's update semantics leave them alone. Modelling the
//! whole thing would turn every field SaaS Fabric does not manage into a field
//! it silently resets.

mod oidc_client;
mod realm;
mod role;
mod token;

pub(crate) use oidc_client::{
    AudienceMapper, ClientRepresentation, NewClientRepresentation, ProtocolMapperRepresentation,
    AUDIENCE_MAPPER_CONFIG_KEY, AUDIENCE_MAPPER_TYPE, PKCE_CHALLENGE_METHOD_ATTRIBUTE,
    POST_LOGOUT_REDIRECT_URIS_ATTRIBUTE,
};
pub(crate) use realm::{NewRealmRepresentation, RealmRepresentation, RealmUpdate};
pub(crate) use role::{NewRoleRepresentation, RoleRepresentation};
pub(crate) use token::TokenResponse;
