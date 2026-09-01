//! The two Git integrations' routes.
//!
//! One set of handlers, mounted twice. Which service a route acts on is decided
//! *here*, by the type it is mounted with — there is no path segment naming an
//! integration and no request body that could carry one, so no caller can reach
//! the flow they were not routed to.

use axum::routing::{get, post, put};
use axum::Router;

use crate::handlers;
use crate::handlers::{ClientConfigurationFlow, PlatformManagementFlow};
use crate::state::ControlPlaneState;

/// Connecting the repository client configuration lives in.
pub(super) fn client_configuration() -> Router<ControlPlaneState> {
    Router::new()
        .route(
            "/integrations/git",
            get(handlers::get_integration).delete(handlers::disconnect::<ClientConfigurationFlow>),
        )
        .route(
            "/integrations/git/connect",
            post(handlers::begin_connection::<ClientConfigurationFlow>),
        )
        .route(
            "/integrations/git/install",
            get(handlers::begin_install::<ClientConfigurationFlow>),
        )
        .route(
            "/integrations/git/repositories",
            get(handlers::list_repositories::<ClientConfigurationFlow>),
        )
        .route(
            "/integrations/git/repository",
            put(handlers::choose_repository::<ClientConfigurationFlow>),
        )
        // The two the Git host redirects a browser to. They take no operator
        // — a redirect carries no bearer — and are correlated by a single-use
        // token instead.
        .route(
            "/integrations/git/created",
            get(handlers::created::<ClientConfigurationFlow>),
        )
        .route(
            "/integrations/git/installed",
            get(handlers::installed::<ClientConfigurationFlow>),
        )
}

/// Connecting the repository desired platform state lives in.
pub(super) fn platform_management() -> Router<ControlPlaneState> {
    Router::new()
        .route(
            "/integrations/platform",
            get(handlers::get_platform_integration).delete(handlers::disconnect::<PlatformManagementFlow>),
        )
        .route(
            "/integrations/platform/connect",
            post(handlers::begin_connection::<PlatformManagementFlow>),
        )
        .route(
            "/integrations/platform/install",
            get(handlers::begin_install::<PlatformManagementFlow>),
        )
        .route(
            "/integrations/platform/repositories",
            get(handlers::list_repositories::<PlatformManagementFlow>),
        )
        .route(
            "/integrations/platform/repository",
            put(handlers::choose_repository::<PlatformManagementFlow>),
        )
        .route(
            "/integrations/platform/created",
            get(handlers::created::<PlatformManagementFlow>),
        )
        .route(
            "/integrations/platform/installed",
            get(handlers::installed::<PlatformManagementFlow>),
        )
}
