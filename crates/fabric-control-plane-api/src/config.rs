//! The control-plane process's configuration.
//!
//! | Module | Responsibility |
//! |---|---|
//! | `app_config` | The struct and its defaults |
//! | `audit_trail` | Where the durable audit trail is stored, if anywhere |
//! | `desired_state` | Which desired-state repository to use |
//! | `identity_provider` | Which identity provider to use |
//! | `env_namespace` | Which environment variables are settings at all |
//! | `loading` | File and environment layering |

mod app_config;
mod audit_trail;
mod desired_state;
mod env_namespace;
mod git_host;
mod identity_provider;
mod loading;
mod platform_management;
mod publication;
mod registries;
mod secret_store;

pub use app_config::ControlPlaneAppConfig;
pub use audit_trail::AuditTrailConfig;
pub use desired_state::DesiredStateConfig;
pub use env_namespace::CONFIG_PATH_VAR;
pub use git_host::GitHostConfig;
pub use identity_provider::IdentityProviderConfig;
pub use platform_management::{PlatformManagementConfig, RegistryBinding};
pub use publication::PublicationConfig;
pub use registries::RegistriesConfig;
pub use secret_store::SecretStoreConfig;
