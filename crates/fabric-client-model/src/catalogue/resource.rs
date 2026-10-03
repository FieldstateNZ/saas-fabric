//! A logical resource one application exposes through the Data API.
//!
//! [`ApplicationResource`] is declared in `fabric-component`, with its
//! wire conversion and its tests, because a component descriptor declares
//! resources in exactly this shape (ADR 0026 section 2). It is re-exported
//! here unchanged, so a stored catalogue reads and renders byte for byte.
pub use fabric_component::ApplicationResource;
