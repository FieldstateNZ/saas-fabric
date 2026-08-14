//! What the Data API returns when it cannot serve a request.
//!
//! Split because the concerns are genuinely different: what went wrong
//! ([`DataApiError`]), what status that maps to (`status_mapping`), and how it
//! is sent (`response`).
//!
//! Connector failures then need two more, because they are the only failures
//! whose answer depends on what the caller was doing at the time:
//! `connector_mapping` classifies one into a status and a code, and
//! `connector_messages` decides what the caller is told. That is where the §29
//! disclosure rules meet the difference between "your write did not happen" and
//! "your write may have happened", and it is worth reading on its own.

mod connector_mapping;
mod connector_messages;
mod data_api_error;
#[cfg(test)]
mod error_tests;
mod response;
mod status_mapping;

pub use data_api_error::DataApiError;
