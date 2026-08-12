//! The HTTP transport to a connector service.

mod error_mapping;
mod http_client;
mod response_decoding;
#[cfg(test)]
mod response_decoding_tests;

pub(crate) use http_client::NdcHttpClient;
