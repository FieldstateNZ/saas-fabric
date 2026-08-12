//! Executing logical data operations for a tenant.
//!
//! The chain the platform owns, split by responsibility:
//!
//! | Module | Responsibility |
//! |---|---|
//! | `data_api_service` | The service and its dependencies |
//! | `prepare` | Catalogue lookup, authorization, and DataSource resolution |
//! | `prepared` | What a prepared operation carries |
//! | `read_operations` | `list` and `read` |
//! | `write_operations` | `create`, `update`, and `delete` |
//! | `row_mapping` | JSON to neutral rows, and the key predicate |

mod data_api_service;
mod prepare;
mod prepared;
mod read_operations;
mod row_mapping;
mod write_operations;

pub use data_api_service::DataApiService;
