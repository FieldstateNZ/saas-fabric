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
//!
//! `prepare` and `authorize` are split because the *order* of the chain and the
//! *policy* it applies fail in different ways and are reviewed by different
//! people. `prepare`'s rustdoc is where the ordering rule lives.

mod authorize;
mod data_api_service;
mod prepare;
mod prepared;
mod read_operations;
mod row_mapping;
mod write_operations;

pub use data_api_service::DataApiService;
