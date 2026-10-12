//! The map from logical resource names to what they actually are.

mod decoder;
mod resource_catalog;
mod resource_definition;

pub use decoder::{decode_catalogue, ResourceCatalogDecoder};
pub use resource_catalog::ResourceCatalog;
pub use resource_definition::ResourceDefinition;
