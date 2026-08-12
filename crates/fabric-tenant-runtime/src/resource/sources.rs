//! Resource sources shipped with the platform.

mod in_memory;
mod json_file;
#[cfg(test)]
mod json_file_tests;

pub use in_memory::InMemorySource;
pub use json_file::JsonFileSource;
