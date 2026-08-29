//! Which repository holds client desired state.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// A repository the installation can reach.
#[derive(Serialize)]
struct Candidate {
    /// The owning account.
    owner: String,

    /// The repository name.
    name: String,

    /// The branch the host considers default.
    default_branch: String,
}

/// Which repository the operator chose.
#[derive(Deserialize)]
pub(crate) struct Chosen {
    /// The owning account.
    owner: String,

    /// The repository name.
    name: String,
}

/// Every repository the installation can reach.
///
/// Read from the host each time rather than from the record. What an
/// installation reaches is the host's to change, and with no webhook to tell
/// this platform when it does, a cached list is a list that is quietly wrong.
pub(crate) async fn list_repositories(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<serde_json::Value>, ControlPlaneError> {
    let repositories = state.git_integration()?.accessible_repositories().await?;

    let candidates: Vec<Candidate> = repositories
        .into_iter()
        .map(|repository| Candidate {
            owner: repository.owner,
            name: repository.name,
            default_branch: repository.default_branch,
        })
        .collect();

    Ok(Json(serde_json::json!({ "repositories": candidates })))
}

/// Settles on the repository client desired state lives in.
pub(crate) async fn choose_repository(
    operator: Operator,
    State(state): State<ControlPlaneState>,
    Json(body): Json<Chosen>,
) -> Result<http::StatusCode, ControlPlaneError> {
    state
        .git_integration()?
        .choose_repository(&operator, &body.owner, &body.name)
        .await?;

    Ok(http::StatusCode::NO_CONTENT)
}
