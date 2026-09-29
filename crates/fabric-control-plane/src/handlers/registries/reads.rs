//! Which registry each managed component is read through (ADR 0026 section
//! 5): per registry host, the images Platform Management reads there, and
//! how each is read now.
//!
//! In the 121–150 line band: the view is a contract the console words from,
//! each field's meaning documented beside it, and the one function that
//! fills it belongs with the fields it fills.

use std::collections::BTreeMap;

use serde::Serialize;

use super::read_by::{read_by, ReadBy};
use crate::registries::Listed;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, DeploymentRegistry};

/// What the listing says about the managed components' reads.
///
/// # Only what was observed
///
/// Each answer is what Fabric read now: the environment's pins from desired
/// state, and the registry records beside them. Nothing says a read
/// succeeded — that is the platform panel's — only where it goes and what it
/// presents. A deployment managing no platform says so, and desired state
/// that could not be read is said with its code, never shown as "no
/// components".
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub(crate) enum ComponentReads {
    /// This deployment manages no platform.
    NotManaged,

    /// The environment's desired state could not be read now.
    Unavailable {
        /// Why, as `GET /api/platform` would answer it.
        code: &'static str,
    },

    /// The environment's image components, by the registry host each image
    /// is read through, in host order.
    Observed {
        /// Every host an image is read from.
        hosts: Vec<HostReads>,
    },
}

/// One registry host, and the images read through it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostReads {
    /// The host, as the repositories name it.
    host: String,

    /// Whether an operator registered a registry for it. With neither this
    /// nor `deployment`, nothing reads it: the host is refused by name.
    registered: bool,

    /// Whether the registry an operator registered for it is being read
    /// through now: `false` for one recorded and not restored since the
    /// last start, or recorded for the deployment's host at another
    /// endpoint. Always `false` when `registered` is `false`.
    installed: bool,

    /// Whether it is the deployment's own registry's host, read anonymously
    /// unless an operator registered it with a credential.
    deployment: bool,

    /// The images read through it, by component and role.
    images: Vec<ImageRead>,
}

/// One image a managed component pins.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageRead {
    /// The component.
    component: String,

    /// The role it names this image by.
    role: String,

    /// The repository.
    repository: String,

    /// Whether the repository is registered under its host's registry. Only
    /// a registered repository is ever presented a credential.
    registered: bool,

    /// How it is read now.
    read: ReadBy,
}

/// The managed components' reads, beside `listed`.
pub(crate) async fn component_reads(
    state: &ControlPlaneState,
    listed: &[Listed],
    deployment: Option<&DeploymentRegistry>,
) -> ComponentReads {
    let Some(platform) = state.platform.as_ref() else {
        return ComponentReads::NotManaged;
    };
    let images = match platform.service.image_repositories(&platform.environment).await {
        Ok(images) => images,
        Err(error) => {
            return ComponentReads::Unavailable {
                code: ControlPlaneError::Platform(error).code(),
            }
        }
    };

    let mut hosts: BTreeMap<String, HostReads> = BTreeMap::new();
    for (component, roles) in images {
        for (role, repository) in roles {
            // The host the router reads a repository through: everything
            // before its first `/`.
            let host = repository.split_once('/').map_or("", |(host, _)| host).to_owned();
            let record = listed.iter().find(|held| held.record.host.as_str() == host);
            let registered = record.is_some_and(|held| {
                held.record
                    .repositories
                    .iter()
                    .any(|registered| registered.repository.as_str() == repository)
            });
            let deployment = deployment.is_some_and(|deployment| deployment.host() == host);
            let read = read_by(record, registered, deployment);
            hosts
                .entry(host.clone())
                .or_insert_with(|| HostReads {
                    registered: record.is_some(),
                    installed: record.is_some_and(|held| held.installed),
                    deployment,
                    host,
                    images: Vec::new(),
                })
                .images
                .push(ImageRead {
                    component: component.clone(),
                    role,
                    repository,
                    registered,
                    read,
                });
        }
    }
    ComponentReads::Observed {
        hosts: hosts.into_values().collect(),
    }
}
