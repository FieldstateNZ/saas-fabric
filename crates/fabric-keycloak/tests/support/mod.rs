//! A Keycloak that answers over a real socket.
//!
//! # Why a socket rather than a mocked client
//!
//! Everything worth testing about this adapter is protocol: whether it asks
//! for the right path, sends the right body, presents a bearer token, and
//! reads a `404` as absence and a `409` as success. A mocked `reqwest::Client`
//! would let all four be wrong and every test still pass.

// Each test binary compiles the whole support module but uses a subset.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod fake_keycloak;

pub use fake_keycloak::{FakeKeycloak, RecordedRequest};
