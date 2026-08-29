//! The Git integration's operator-facing surface.
//!
//! # Two of these handlers take no operator, and that is the design
//!
//! The Git host returns the operator's *browser* to this API after each
//! approval. That navigation carries no bearer token — the console holds its
//! token in memory and cannot attach it to a redirect it did not make — so the
//! callbacks cannot demand an `Operator`.
//!
//! What they demand instead is a correlation token this platform issued to an
//! authenticated operator moments earlier, which is single-use and expires.
//! See [`PendingFlows`](crate::git_integration::PendingFlows) for why that is
//! held server-side rather than signed.

mod callbacks;
mod connect;
mod repositories;
mod status;

pub(crate) use callbacks::{created, installed};
pub(crate) use connect::{begin_connection, begin_install, disconnect};
pub(crate) use repositories::{choose_repository, list_repositories};
pub(crate) use status::get_integration;
