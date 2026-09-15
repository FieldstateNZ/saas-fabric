//! A loopback-only workbench for developing the operator console against a
//! real control plane, without a Keycloak, a Git host, or an identity
//! provider to stand up first.
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
//!    the local development repository it also owns.
//! 3. **It is an `[[example]]`, never a `[[bin]]`.** The `Dockerfile` at the
//!    repository root builds `cargo build --release --bin ...` for named
//!    binaries only; an example is not a build target that command touches,
//!    so this code cannot end up in a shipped image by a `Dockerfile` change
//!    that forgets to exclude it — there is no line to forget.

use fabric_control_plane::{
    build_control_plane, testing::AcceptingOperator, ControlPlaneConfig, ControlPlaneDeps,
    DesiredStateBinding, KeyHolder,
};
use fabric_control_plane_api::local_repository::LocalClientRepository;
use fabric_core::SystemClock;
use std::{path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::var_os("FABRIC_WORKBENCH_DATA")
        .map_or_else(|| PathBuf::from(".local/workbench"), PathBuf::from);
    let repository = LocalClientRepository::open(&directory).await?;
    let config: ControlPlaneConfig = serde_json::from_value(serde_json::json!({
        "operator": {"mode":"oidc", "issuer":"https://auth.example.test/realms/master",
            "redirect_uri":"http://127.0.0.1:5174/"}
    }))?;
    let services = build_control_plane(
        &config,
        ControlPlaneDeps {
            platform: None,
            platform_integration: None,
            client_secrets: None,
            desired_state: DesiredStateBinding::to(Arc::new(repository)),
            clock: SystemClock::shared(),
            keys: KeyHolder::empty(),
            identity_provider: None,
            sign_in: None,
            git_integration: None,
            operators: Some(AcceptingOperator::accepting("local-workbench")),
        },
    )?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8082").await?;
    eprintln!(
        "Local workbench API: http://127.0.0.1:8082; storage {}",
        directory.display()
    );
    axum::serve(listener, services.router).await?;
    Ok(())
}
