//! Talking to Keycloak's admin API.

mod errors;
#[cfg(test)]
mod errors_tests;
mod http;
mod paths;
mod requests;
mod token;

pub(crate) use http::KeycloakAdmin;
pub(crate) use paths::Paths;
