//! What the platform knows about its Git integration.

use serde::{Deserialize, Serialize};

/// The repository this platform reads and writes client desired state in.
///
/// Chosen during the install flow rather than stated by a deployment. The
/// brief is explicit that repository selection must not become an
/// irreversible domain assumption, so it is data — an operator can install the
/// App somewhere else and the platform follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedRepository {
    /// The account or organisation that owns it.
    pub owner: String,

    /// The repository name.
    pub name: String,

    /// The branch desired state is read from and written to.
    pub branch: String,

    /// The directory each client's document lives under.
    pub path_prefix: String,

    /// The file within a client's directory.
    pub document_file: String,
}

impl SelectedRepository {
    /// The platform's convention, applied to a repository an operator chose.
    ///
    /// The layout is a convention rather than a setting because both sides of
    /// it are this platform's: it writes these documents and it reads them.
    /// Making it configurable would invite two deployments to disagree about
    /// where a client lives in the same repository.
    #[must_use]
    pub fn conventional(
        owner: impl Into<String>,
        name: impl Into<String>,
        branch: impl Into<String>,
    ) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
            branch: branch.into(),
            path_prefix: "clients".to_owned(),
            document_file: "client.yaml".to_owned(),
        }
    }

    /// How this repository is named to an operator.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// The durable record of this platform's Git integration.
///
/// # Two stages, and the gap between them is real
///
/// Creating the App and installing it are separate approvals on GitHub, and an
/// operator can complete the first and abandon the second. `installation` is
/// therefore optional, and a record with `None` is not a broken record — it is
/// a platform that owns an App nobody has installed yet, which is a state the
/// console has to be able to show and the flow has to be able to resume.
///
/// **No credential is in here.** The App's private key goes to the secret
/// partition; this holds only what is safe to write down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitIntegration {
    /// The App's numeric identifier, which becomes its JWT's issuer.
    pub app_id: String,

    /// The App's URL slug, needed to build its install page.
    pub app_slug: String,

    /// The installation, once the App has been installed somewhere.
    pub installation: Option<Installation>,
}

/// An installation of the App, and what it gave the platform access to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installation {
    /// The installation's identifier, which tokens are minted against.
    pub id: String,

    /// The account the App was installed on.
    pub account: String,

    /// The repository desired state lives in, once one has been settled on.
    ///
    /// `None` when the installation grants access to more than one repository
    /// and nobody has chosen yet. The platform declines to guess: picking the
    /// wrong repository would write client configuration somewhere nobody
    /// expects, and it would look like it worked.
    pub repository: Option<SelectedRepository>,
}

impl GitIntegration {
    /// A newly created App, not yet installed anywhere.
    #[must_use]
    pub fn created(app_id: impl Into<String>, app_slug: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            app_slug: app_slug.into(),
            installation: None,
        }
    }

    /// The repository this integration reads and writes, if it has settled on
    /// one.
    #[must_use]
    pub fn repository(&self) -> Option<&SelectedRepository> {
        self.installation.as_ref()?.repository.as_ref()
    }

    /// Whether this integration can actually be used to reach desired state.
    ///
    /// Both halves are required, and neither implies the other: an App with no
    /// installation can mint nothing, and an installation with no repository
    /// has nowhere to read.
    #[must_use]
    pub fn is_usable(&self) -> bool {
        self.repository().is_some()
    }
}
