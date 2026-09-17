//! A deployment-owned binding, never accepted from an API caller.
use serde::Deserialize;

/// One named container whose deployment evidence an operator may read.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadTarget {
    /// Kubernetes namespace, used only inside this adapter.
    pub namespace: String,
    /// Exact deployment name; RBAC can restrict `get` to this resource.
    pub deployment: String,
    /// Container to inspect, excluding sidecars from release version agreement.
    pub container: String,
    /// Expected image repository, without tag or digest.
    pub repository: String,
}

impl WorkloadTarget {
    pub(crate) fn validate(&self) -> Result<(), String> {
        for value in [&self.namespace, &self.deployment, &self.container] {
            if value.is_empty()
                || value.len() > 253
                || !value
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'.')
            {
                return Err(
                    "observation targets require DNS-style namespace, deployment and container names".into(),
                );
            }
        }
        if self.repository.is_empty()
            || self.repository.contains(['@', '?', '#'])
            || self.repository.chars().any(char::is_whitespace)
        {
            return Err("observation targets require an exact image repository".into());
        }
        Ok(())
    }
}
