//! Explicit loopback-only UI workbench: real storage/API, test operator, no providers.
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
