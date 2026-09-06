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
//!
//! **Caveat, observed against a real Keycloak 26.0.8:** "leave them alone" is
//! Keycloak's usual behaviour, not a guarantee for every field. `PUT
//! /clients/{id}` was seen to flip `frontchannelLogout` to its own default on
//! the *first* write to a client that had never carried the key. Nothing in
//! this crate's model reads `frontchannelLogout`, so the flip changes no
//! decision this adapter or the reconciler makes, and the second `PUT` of the
//! same declaration is byte-stable — Keycloak does not flip it again once it
//! has a value. Recorded in `docs/verification.md`, "Keycloak 26.0.8 probe,
//! 2026-09-06 (issue #61)", finding 7.

mod oidc_client;
mod protocol_mapper;
mod realm;
mod role;
mod token;

pub(crate) use oidc_client::{
    ClientRepresentation, NewClientRepresentation, PKCE_CHALLENGE_METHOD_ATTRIBUTE,
    POST_LOGOUT_REDIRECT_URIS_ATTRIBUTE,
};
pub(crate) use protocol_mapper::{
    AudienceMapper, ProtocolMapperRepresentation, AUDIENCE_MAPPER_CONFIG_KEY, AUDIENCE_MAPPER_TYPE,
};
pub(crate) use realm::{NewRealmRepresentation, RealmRepresentation, RealmUpdate};
pub(crate) use role::{NewRoleRepresentation, RoleRepresentation};
pub(crate) use token::TokenResponse;
