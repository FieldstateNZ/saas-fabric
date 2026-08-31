//! Talking to the Git host's contents API.

mod contents;
mod decoding;
mod errors;
#[cfg(test)]
mod errors_tests;
mod http;
mod operations;
mod sending;
mod wire;

pub(crate) use http::GitHost;
