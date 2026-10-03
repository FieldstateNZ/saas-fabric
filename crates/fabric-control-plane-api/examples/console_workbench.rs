//! A loopback-only workbench for developing the operator console against a
//! real control plane, without a Keycloak, a Git host, or an identity
//! provider to stand up first.
//!
//! Past 120 lines because why it is safe to run is argued beside the code.
//!
//! # What is real, and what is not
//!
//! The router, the desired-state repository ([`LocalClientRepository`]) and
//! every handler are exactly what a deployment runs. What is not real is who
//! is allowed to call them: `operators` is
//! [`AcceptingOperator`](fabric_control_plane::testing::AcceptingOperator),
//! the same test-only authenticator `crate::testing` documents, wired here to
//! a binary instead of a test harness.
//!
//! # Why that is not a hole in `testing`'s "nothing reaches a deployment"
//!
//! Three things hold at once, and all three are required:
//!
//! 1. **It listens on `127.0.0.1` only.** Nothing off this machine can reach
//!    it, with or without a credential.
//! 2. **It has no identity provider, no Git integration and no secrets
//!    store.** `ControlPlaneDeps` for every one of those is `None`, so there
//!    is nothing behind this process for an accepted request to reach beyond
//!    the local development repository it also owns — and the image
//!    registries, which are the real service over in-memory stores and the
//!    real router: an operator registers public registries here and sees what
//!    was proven. They read only public addresses, lose everything when this
//!    process stops, and no credential typed here is written anywhere else.
//! 3. **It is an `[[example]]`, never a `[[bin]]`.** The `Dockerfile` at the
//!    repository root builds `cargo build --release --bin ...` for named
//!    binaries only; an example is not a build target that command touches,
//!    so this code cannot end up in a shipped image by a `Dockerfile` change
//!    that forgets to exclude it — there is no line to forget.
//!
//! # Why it still checks `Host`, bound to loopback or not
//!
//! Binding to `127.0.0.1` keeps a network attacker out, but not a browser
//! already running on this machine. DNS rebinding is exactly the gap: a page
//! on some other origin points a hostname it controls at `127.0.0.1`, the
//! browser resolves it and connects here, and the request arrives carrying
//! that attacker's own choice of `Host` header — nothing about "only
//! reachable from this machine" stops it, because the request genuinely did
//! come from this machine. [`reject_unexpected_host`] is the one thing
//! standing between that page and this loopback-only API.

use axum::extract::Request;
use axum::http::{header::HOST, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use fabric_control_plane::{
    build_control_plane, testing::AcceptingOperator, ControlPlaneConfig, ControlPlaneDeps,
    DesiredStateBinding, InMemoryRegistryStore, InMemorySecretStore, KeyHolder,
};
use fabric_control_plane_api::local_repository::LocalClientRepository;
use fabric_control_plane_api::startup::{compose_registries, RegistryComposition};
use fabric_core::SystemClock;
use std::{path::PathBuf, sync::Arc};

/// The address this workbench serves — the only `Host` header it accepts,
/// in either of the two spellings a browser might send for it.
const ADDRESS: &str = "127.0.0.1:8082";

/// The `Host` spellings [`reject_unexpected_host`] admits.
const ALLOWED_HOSTS: [&str; 2] = [ADDRESS, "localhost:8082"];

/// Refuses a request whose `Host` header names anything but this workbench's
/// own address — see this module's own doc for why a loopback bind alone
/// does not already guarantee that.
async fn reject_unexpected_host(request: Request, next: Next) -> Response {
    let host = request.headers().get(HOST).and_then(|value| value.to_str().ok());

    if host.is_some_and(|host| ALLOWED_HOSTS.contains(&host)) {
        next.run(request).await
    } else {
        (StatusCode::MISDIRECTED_REQUEST, "this Host is not served here").into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::var_os("FABRIC_WORKBENCH_DATA")
        .map_or_else(|| PathBuf::from(".local/workbench"), PathBuf::from);
    let repository = LocalClientRepository::open(&directory).await?;
    let config: ControlPlaneConfig = serde_json::from_value(serde_json::json!({
        "operator": {"mode":"oidc", "issuer":"https://auth.example.test/realms/master",
            "redirect_uri":"http://127.0.0.1:5174/"}
    }))?;
    // The real registry service and router, over stores that forget. No
    // deployment registry: the workbench manages no platform.
    let registries = compose_registries(RegistryComposition {
        deployment: None,
        http_timeout_seconds: 10,
        resolution_budget_seconds: 8,
        store: Arc::new(InMemoryRegistryStore::new()),
        secrets: Arc::new(InMemorySecretStore::new()),
        clock: SystemClock::shared(),
    })
    .await?;
    let services = build_control_plane(
        &config,
        ControlPlaneDeps {
            platform: None,
            platform_integration: None,
            publication: None,
            client_secrets: None,
            desired_state: DesiredStateBinding::to(Arc::new(repository)),
            clock: SystemClock::shared(),
            keys: KeyHolder::empty(),
            identity_provider: None,
            sign_in: None,
            git_integration: None,
            operators: Some(AcceptingOperator::accepting("local-workbench")),
            // No Keycloak sits behind the workbench, so there is nothing to
            // protect a name against — an empty set is honest, not a gap.
            reserved_realms: std::collections::BTreeSet::new(),
            reserved_client_ids: std::collections::BTreeSet::new(),
            resolution: registries.resolution(),
            registries: registries.service,
        },
    )?;
    let router = services.router.layer(middleware::from_fn(reject_unexpected_host));

    let listener = tokio::net::TcpListener::bind(ADDRESS).await?;
    eprintln!(
        "Local workbench API: http://{ADDRESS}; storage {}",
        directory.display()
    );
    axum::serve(listener, router).await?;
    Ok(())
}
