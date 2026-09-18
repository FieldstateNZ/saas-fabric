//! What an operator sees and does about a client's data placements
//! (ADR 0023 part 2).
//!
//! A client's `spec.data.<logical>` is intent; placing it is a Fabric
//! write, delegated whole to `fabric_platform_management::Placements`.
//! Nothing here decides anything -- these handlers read a client, convert
//! its intent into the one the selector reads (`intent.rs`), and render
//! what the service answers.

mod body;
mod get;
mod intent;
mod post;

pub(crate) use get::list_placements;
pub(crate) use post::place_data_source;
