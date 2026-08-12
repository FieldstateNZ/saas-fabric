//! The whole process's configuration.
//!
//! Four files, one per responsibility:
//!
//! | Module | Responsibility |
//! |---|---|
//! | `app_config` | The struct and its defaults |
//! | `token_config` | The identity posture, which is its own decision |
//! | `loading` | File and environment layering |
//! | `validation` | Checks that span more than one domain |
//!
//! `loading` and `validation` are `impl AppConfig` blocks in their own modules
//! rather than methods on the struct's file. Rust keeps a type with its impls,
//! but it does not require every impl in one place — and parsing and validation
//! are genuinely different concerns from the shape being parsed.

mod app_config;
mod loading;
mod token_config;
mod validation;
#[cfg(test)]
mod validation_tests;

pub use app_config::AppConfig;
pub use token_config::TokenConfig;
