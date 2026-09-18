//! What an operator declares about an environment's data sources.
//!
//! ADR 0023 part 1: a data source is environment desired state, declared
//! through the console the same way a client or the product catalogue is —
//! read, corrected with a precondition, and written in one commit by
//! Platform Management. Nothing here places a tenant; that is part 2, and
//! is not built yet.

mod body;
mod placement;
mod request;

mod get;
mod put;

pub(crate) use get::list_data_sources;
pub(crate) use put::declare_data_source;
