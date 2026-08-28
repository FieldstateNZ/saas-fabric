//! Event categories used when composing a structured event ID.

/// The category of a logged event.
///
/// Each variant occupies a hundred-wide range inside a domain's thousand-wide
/// ID space, so the category is readable straight off the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum EventType {
    /// The operation completed as intended.
    Success = 0,
    /// Input failed validation.
    Validation = 1,
    /// The operation failed and needs attention.
    Error = 2,
    /// A recoverable problem, such as a retry or a miss that has a fallback.
    Warning = 3,
    /// Internal state useful during development.
    Debug = 4,
    /// Granular diagnostic detail.
    Trace = 5,
}
