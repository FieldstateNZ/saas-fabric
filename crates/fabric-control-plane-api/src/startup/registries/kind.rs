//! Which settings a recorded registry's kind builds it with.

use fabric_control_plane::{RealmOrigin, RegistryConnection, RegistryKind};
use fabric_registry::{RealmRule, RegistrySettings};

/// The settings a connection's kind builds, and its realm rule.
pub(super) fn by_kind(connection: &RegistryConnection) -> Result<RegistrySettings, String> {
    Ok(match connection.kind {
        RegistryKind::Ghcr => RegistrySettings::ghcr(),
        RegistryKind::DockerHub => RegistrySettings::docker_hub(),
        RegistryKind::Distribution => {
            let realm = match &connection.realm {
                RealmOrigin::Follow => RealmRule::FollowChallenge,
                RealmOrigin::Recorded(origin) => RealmRule::Recorded {
                    origin: origin.clone(),
                },
            };
            RegistrySettings::distribution(&connection.endpoint, realm)?
        }
    })
}

/// Whether two endpoints name one origin, ignoring a trailing `/`.
pub(super) fn same(left: &str, right: &str) -> bool {
    left.trim_end_matches('/') == right.trim_end_matches('/')
}
