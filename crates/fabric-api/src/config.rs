//! The whole process's configuration.
//!
//! One file per responsibility:
//!
//! | Module | Responsibility |
//! |---|---|
//! | `app_config` | The struct and its defaults |
//! | `token_config` | The identity posture, which is its own decision |
//! | `allowlist` | A claim allowlist that cannot be empty |
//! | `administrator_role` | Refusing a role name that authorises everyone |
//! | `env_namespace` | Which environment variables are settings at all |
//! | `loading` | File and environment layering |
//! | `load_failure` | Saying which of the two sources failed |
//! | `validation` | Checks that span more than one domain |
//!
//! `loading` and `validation` are `impl AppConfig` blocks in their own modules
//! rather than methods on the struct's file. Rust keeps a type with its impls,
//! but it does not require every impl in one place — and parsing and validation
//! are genuinely different concerns from the shape being parsed.

mod administrator_role;
mod allowlist;
mod app_config;
mod env_namespace;
mod load_failure;
mod loading;
mod token_config;
mod validation;
#[cfg(test)]
mod validation_tests;

pub use allowlist::Allowlist;
pub use app_config::AppConfig;
pub use env_namespace::CONFIG_PATH_VAR;
pub use token_config::TokenConfig;
