//! How one managed component's image is read now.

use serde::Serialize;

use crate::registries::{CredentialState, Listed};

/// How an image is read now: one closed answer, so the console words each
/// and none reads as another.
///
/// # Only what Fabric presents
///
/// This says what a read of the image would present, from the registry's
/// record and its client as they are now — never that a read succeeded,
/// which is the platform panel's to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ReadBy {
    /// With the registry's credential: the repository is registered under a
    /// registry being read through, whose credential is held, was readable
    /// at the last start, and has not been refused.
    Credential,

    /// Anonymously: through a registry being read through that presents no
    /// credential for this repository, or through the deployment's own.
    Anonymous,

    /// Not read: the repository is registered under a registry whose realm
    /// refused its credential, which is not presented again until it is
    /// replaced, and every read that would present it fails without asking.
    CredentialRefused,

    /// Not read now: the host's registry is recorded and not being read
    /// through, or no registry reads the host at all, and it is not the
    /// deployment's host.
    NotRead,
}

/// How an image on `record`'s host is read: `registered` says whether its
/// repository is registered there, and `deployment` whether the host is the
/// deployment's own registry's.
pub(super) fn read_by(record: Option<&Listed>, registered: bool, deployment: bool) -> ReadBy {
    let Some(held) = record.filter(|held| held.installed) else {
        // No operator's registry reads the host now: only the deployment's
        // own, anonymously, if it is the deployment's host.
        return if deployment {
            ReadBy::Anonymous
        } else {
            ReadBy::NotRead
        };
    };
    if !registered || held.record.credential.is_none() {
        return ReadBy::Anonymous;
    }
    match held.credential {
        CredentialState::Presented => ReadBy::Credential,
        CredentialState::Refused => ReadBy::CredentialRefused,
        CredentialState::Unreadable => ReadBy::Anonymous,
    }
}
