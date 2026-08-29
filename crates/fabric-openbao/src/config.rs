//! What a deployment states about its secret store.

/// How to reach OpenBao, and where this instance's partition is.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenBaoConfig {
    /// The API address, such as `http://openbao.secrets.svc.cluster.local:8200`.
    pub address: String,

    /// The KV version 2 mount secrets are kept under.
    #[serde(default = "default_mount")]
    pub mount: String,

    /// This Fabric instance's partition within that mount.
    ///
    /// Every name the control plane asks for is resolved beneath this, which
    /// is what makes the partition a partition: an instance cannot name its
    /// way out of its own prefix, because it never supplies a prefix.
    #[serde(default = "default_prefix")]
    pub prefix: String,

    /// The Kubernetes auth mount to log in against.
    #[serde(default = "default_auth_mount")]
    pub auth_mount: String,

    /// The role to log in as.
    pub role: String,

    /// Where the pod's own service-account token is mounted.
    ///
    /// Stated rather than hard-coded because it is a property of how the pod
    /// is configured, and a deployment that projects a differently-audienced
    /// token elsewhere should not have to patch the application.
    #[serde(default = "default_token_path")]
    pub service_account_token_path: String,

    /// How long a request may take.
    #[serde(default = "default_timeout")]
    pub http_timeout_seconds: u64,
}

/// The conventional KV version 2 mount.
fn default_mount() -> String {
    "secret".to_owned()
}

/// The master instance's partition.
fn default_prefix() -> String {
    "platform/saas-fabric/instances/master".to_owned()
}

/// The conventional Kubernetes auth mount.
fn default_auth_mount() -> String {
    "kubernetes".to_owned()
}

/// Where Kubernetes projects a pod's service-account token by default.
fn default_token_path() -> String {
    "/var/run/secrets/kubernetes.io/serviceaccount/token".to_owned()
}

/// Ten seconds, matching the other platform clients.
fn default_timeout() -> u64 {
    10
}
