//! The HTTP transport to a connector service.

mod error_mapping;
mod http_client;

pub(crate) use http_client::NdcHttpClient;
