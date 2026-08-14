//! The HTTP transport to a connector service.

#[cfg(test)]
mod delivery_tests;
mod error_mapping;
#[cfg(test)]
mod fake_connector;
mod http_client;
mod response_decoding;
#[cfg(test)]
mod response_decoding_tests;

pub(crate) use http_client::NdcHttpClient;
