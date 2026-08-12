//! Binding sources shipped with the platform.

mod file;
mod in_memory;

pub use file::FileBindingSource;
pub use in_memory::InMemoryBindingSource;
