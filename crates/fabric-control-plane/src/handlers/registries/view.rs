//! What an operator is shown about a registry: what was proven, never
//! "connected", and never a secret.

use serde::Serialize;

use crate::registries::{CredentialState, Listed};
use crate::RegistryKind;

/// One registry.
///
/// # What it says, and what it never says
///
/// Its kind, host and endpoint, the realm origin its proof recorded, whether
/// a credential is held — by which account, set by whom and when, and
/// whether its realm last refused it — and its repositories with when each
/// was proven. Never the token, and never the id that names it in the
/// secret partition.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegistryView {
    /// What it is known by.
    host: String,
    /// Its kind.
    kind: RegistryKind,
    /// Where it is served.
    endpoint: String,
    /// The realm origin its challenge named when it was proven.
    realm_origin: Option<String>,
    /// Whether this is the deployment's host, contributing a credential and
    /// repositories to the deployment's registry.
    deployment: bool,
    /// Whether it is being read through now. `false` only when a restart
    /// could not rebuild it; the startup log says why.
    installed: bool,
    /// Who holds its credential, when one is held.
    credential: Option<CredentialView>,
    /// The operator who registered it.
    registered_by: String,
    /// When, in Unix seconds.
    registered_at: u64,
    /// Its repositories.
    repositories: Vec<RepositoryView>,
}

/// Who holds a registry's credential. Never the token.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CredentialView {
    /// The registry account.
    username: String,
    /// The operator who set it.
    set_by: String,
    /// When, in Unix seconds.
    set_at: u64,
    /// Whether the realm refused it: it is not presented again until it is
    /// replaced or proven again.
    refused: bool,
    /// Whether it could not be read at the last restart, so the registry is
    /// read anonymously until it is set again.
    unreadable: bool,
}

/// A registered repository.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryView {
    /// Its full name.
    repository: String,
    /// When its tag listing last answered, in Unix seconds.
    proven_at: u64,
}

impl RegistryView {
    /// The view of one listed registry.
    pub(crate) fn of(listed: Listed) -> Self {
        let Listed {
            record,
            installed,
            credential: state,
            deployment,
        } = listed;
        Self {
            host: record.host.to_string(),
            kind: record.kind,
            endpoint: record.endpoint,
            realm_origin: record.realm_origin,
            deployment,
            installed,
            credential: record.credential.map(|held| CredentialView {
                username: held.username,
                set_by: held.set_by,
                set_at: held.set_at,
                refused: state == CredentialState::Refused,
                unreadable: state == CredentialState::Unreadable,
            }),
            registered_by: record.registered_by,
            registered_at: record.registered_at,
            repositories: record
                .repositories
                .into_iter()
                .map(|registered| RepositoryView {
                    repository: registered.repository.to_string(),
                    proven_at: registered.proven_at,
                })
                .collect(),
        }
    }
}
