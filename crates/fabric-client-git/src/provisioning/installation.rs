//! Proving an installation works, and reporting what it can reach.

mod sending;

use fabric_control_plane::{AccessibleRepository, InstallationDetail, ProvisioningError, SecretValue};
use serde::Deserialize;

use crate::github::tokens::assertion;
use crate::provisioning::installation::sending::{get, post};
use crate::provisioning::urlencode_path;

/// The account an application is installed on.
#[derive(Deserialize)]
struct InstallationAccount {
    /// The owning account.
    account: Option<Account>,
}

/// A GitHub account, as far as this needs it.
#[derive(Deserialize)]
struct Account {
    /// The account's name.
    login: String,
}

/// A minted installation token.
#[derive(Deserialize)]
struct Minted {
    /// The token itself.
    token: String,
}

/// The repositories an installation can reach.
#[derive(Deserialize)]
struct Reachable {
    /// The repositories.
    repositories: Vec<Repository>,
}

/// One repository.
#[derive(Deserialize)]
struct Repository {
    /// Its name within the owner.
    name: String,

    /// The branch the host considers default.
    default_branch: String,

    /// The owning account.
    owner: Account,
}

/// Mints a token for the installation, then reports what it reaches.
///
/// **The mint comes first and is the point.** An installation is only recorded
/// once a token has been obtained for it, so a recorded installation always
/// means a working one and no separate verified flag has to be kept in step.
///
/// # Errors
///
/// Returns [`ProvisioningError::Refused`] if the host will not mint a token —
/// the installation was removed, suspended, or never belonged to this
/// application — and [`ProvisioningError::Unavailable`] if it cannot be
/// reached.
pub(super) async fn inspect(
    http: &reqwest::Client,
    api_base_url: &str,
    app_id: &str,
    private_key: &SecretValue,
    installation_id: &str,
) -> Result<InstallationDetail, ProvisioningError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ProvisioningError::Unavailable)?
        .as_secs();

    let jwt = assertion::build(app_id, private_key.expose(), now).map_err(|_| ProvisioningError::Refused)?;

    let id = urlencode_path(installation_id);

    let account: InstallationAccount =
        get(http, &format!("{api_base_url}/app/installations/{id}"), &jwt).await?;

    let minted: Minted = post(
        http,
        &format!("{api_base_url}/app/installations/{id}/access_tokens"),
        &jwt,
    )
    .await?;

    let reachable: Reachable = get(
        http,
        &format!("{api_base_url}/installation/repositories?per_page=100"),
        &minted.token,
    )
    .await?;

    Ok(InstallationDetail {
        account: account.account.map(|owner| owner.login).unwrap_or_default(),
        repositories: reachable
            .repositories
            .into_iter()
            .map(|repository| AccessibleRepository {
                owner: repository.owner.login,
                name: repository.name,
                default_branch: repository.default_branch,
            })
            .collect(),
    })
}
