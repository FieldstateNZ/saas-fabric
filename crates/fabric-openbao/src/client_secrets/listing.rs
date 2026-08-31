//! Walking a client's namespace to find every secret in it.
//!
//! The store lists one level at a time and marks a directory with a trailing
//! slash, so finding every path means descending. Bounded, because a listing
//! is a request an operator makes constantly and a pathological tree must not
//! become an unbounded number of calls.

use fabric_control_plane::{SecretNamespace, SecretPath, SecretsError};

use super::OpenBaoClientSecrets;

/// How deep a walk will go.
///
/// Secrets nested more than this are not hidden — they are a sign the
/// convention has drifted, and the console showing a truncated tree is a
/// better failure than a request that never returns.
const MAX_DEPTH: usize = 8;

/// Every secret path in a client's namespace.
pub(super) async fn walk(
    secrets: &OpenBaoClientSecrets,
    namespace: &SecretNamespace,
) -> Result<Vec<SecretPath>, SecretsError> {
    let mut found = Vec::new();
    let mut pending = vec![(String::new(), 0_usize)];

    while let Some((prefix, depth)) = pending.pop() {
        for entry in secrets.entries_at(namespace, &prefix).await? {
            let full = format!("{prefix}{entry}");

            if let Some(directory) = full.strip_suffix('/') {
                if depth < MAX_DEPTH {
                    pending.push((format!("{directory}/"), depth + 1));
                }

                continue;
            }

            // A path the store holds but this platform would not have written.
            // Skipped rather than surfaced: the console can only address paths
            // it can also write, and offering one it cannot is worse than
            // omitting it.
            if let Ok(path) = SecretPath::parse(&full) {
                found.push(path);
            }
        }
    }

    found.sort();

    Ok(found)
}
