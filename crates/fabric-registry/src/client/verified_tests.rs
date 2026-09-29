//! The verified-bytes cache's bounds.

use super::digest::Content;
use super::verified::{Verified, MOST_BYTES, MOST_ENTRIES};

fn content(seed: usize, length: usize) -> Content {
    let mut bytes = seed.to_le_bytes().to_vec();
    bytes.resize(length.max(bytes.len()), b'x');
    Content::hashed(bytes)
}

#[test]
fn held_bytes_come_back_under_their_digest() {
    let cache = Verified::default();
    let held = content(1, 10);
    cache.insert(&held);

    let found = cache.get(&held.digest).expect("held");
    assert_eq!(found.bytes, held.bytes);
    assert!(cache.get(&content(2, 10).digest).is_none());
}

#[test]
fn past_the_entry_bound_the_oldest_goes_first() {
    let cache = Verified::default();
    let all: Vec<Content> = (0..=MOST_ENTRIES).map(|seed| content(seed, 8)).collect();
    for each in &all {
        cache.insert(each);
    }

    assert!(cache.get(&all[0].digest).is_none(), "the oldest is evicted");
    assert!(cache.get(&all[1].digest).is_some());
    assert!(cache.get(&all[MOST_ENTRIES].digest).is_some());
}

#[test]
fn past_the_byte_bound_the_oldest_goes_first() {
    let cache = Verified::default();
    let third = MOST_BYTES / 3;
    let all: Vec<Content> = (0..4).map(|seed| content(seed, third)).collect();
    for each in &all {
        cache.insert(each);
    }

    assert!(cache.get(&all[0].digest).is_none(), "the oldest is evicted");
    assert!(cache.get(&all[3].digest).is_some());
}

#[test]
fn bytes_larger_than_the_whole_budget_are_not_held() {
    let cache = Verified::default();
    let huge = content(0, MOST_BYTES + 1);
    cache.insert(&huge);

    assert!(cache.get(&huge.digest).is_none());
}
