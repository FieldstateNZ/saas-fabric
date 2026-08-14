//! Executing logical data operations for a tenant.
//!
//! The chain the platform owns, split by responsibility:
//!
//! | Module | Responsibility |
//! |---|---|
//! | `data_api_service` | The service and its dependencies |
//! | `prepare` | The ordered chain: catalogue, authorization, resolution |
//! | `authorize` | Applying the authorization policy, and nothing else |
//! | `prepared` | What a prepared operation carries |
//! | `read_operations` | `list` and `read` |
//! | `write_operations` | `create`, `update`, and `delete` |
//! | `row_mapping` | JSON to neutral rows, and the key predicate |
//! | `dispatch_write` | The one path every mutation takes to a connector |
//! | `write_integrity` | Whether a backend's answer agrees with the write sent |
//!
//! `prepare` and `authorize` are split because the *order* of the chain and the
//! *policy* it applies fail in different ways and are reviewed by different
//! people. `prepare`'s rustdoc is where the ordering rule lives.
//!
//! `write_integrity` is split from `write_operations` for a similar reason: it
//! is the one place that decides whether a write may be *called* a success, and
//! the reasoning behind that — which capabilities do not help, and what stays
//! unknowable — needs room to be read on its own.

mod authorize;
mod data_api_service;
mod dispatch_write;
mod prepare;
mod prepared;
mod read_operations;
mod row_mapping;
mod write_integrity;
mod write_operations;

pub use data_api_service::DataApiService;
