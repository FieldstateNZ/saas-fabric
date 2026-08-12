//! What the Data API returns when it cannot serve a request.
//!
//! Split three ways because the concerns are genuinely different: what went
//! wrong ([`DataApiError`]), what status that maps to (`status_mapping`), and
//! what the caller is allowed to be told (`response`). The last one is where
//! the §29 rules live, and it is worth being able to read it on its own.

mod data_api_error;
#[cfg(test)]
mod error_tests;
mod response;
mod status_mapping;

pub use data_api_error::DataApiError;
