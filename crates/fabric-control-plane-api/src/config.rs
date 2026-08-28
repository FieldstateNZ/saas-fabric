//! The control-plane process's configuration.
//!
//! | Module | Responsibility |
//! |---|---|
//! | `app_config` | The struct and its defaults |
//! | `desired_state` | Which desired-state repository to use |
//! | `identity_provider` | Which identity provider to use |
//! | `env_namespace` | Which environment variables are settings at all |
//! | `loading` | File and environment layering |

mod app_config;
mod desired_state;
mod env_namespace;
mod identity_provider;
mod loading;

pub use app_config::ControlPlaneAppConfig;
pub use desired_state::DesiredStateConfig;
pub use env_namespace::CONFIG_PATH_VAR;
pub use identity_provider::IdentityProviderConfig;
