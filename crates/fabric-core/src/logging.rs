//! The structured event-ID scheme shared by every domain crate.
//!
//! Event IDs are stable numbers that survive log-message rewording. Alerting
//! rules key off the number, so changing the wording of a log line never
//! silently breaks a dashboard.

mod event_type;

pub use event_type::EventType;

/// Builds an event ID from a domain, a category, and an event number.
///
/// The format is `(domain_id * 1000) + (event_type * 100) + number`, which
/// gives each domain a thousand-wide block and each category a hundred-wide
/// block inside it. Reading `3201` back tells you: domain 3, an error, event 1.
///
/// # Examples
///
/// ```
/// use fabric_core::{event_id, EventType};
///
/// // Tenant runtime is domain 3; this is its first error event.
/// assert_eq!(event_id(3, EventType::Error, 1), 3201);
/// ```
#[must_use]
pub const fn event_id(domain_id: u32, event_type: EventType, number: u32) -> u32 {
    domain_id * 1000 + (event_type as u32) * 100 + number
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_occupy_distinct_hundred_wide_blocks() {
        assert_eq!(event_id(1, EventType::Success, 1), 1001);
        assert_eq!(event_id(1, EventType::Warning, 1), 1301);
        assert_eq!(event_id(1, EventType::Trace, 1), 1501);
    }

    #[test]
    fn domains_occupy_distinct_thousand_wide_blocks() {
        assert_eq!(event_id(2, EventType::Success, 1), 2001);
        assert_eq!(event_id(3, EventType::Success, 1), 3001);
    }
}
