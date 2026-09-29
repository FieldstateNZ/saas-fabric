//! Bytes this client has already hashed, held by digest.
//!
//! # Content, not answers
//!
//! What a digest names cannot change: bytes that hashed to `sha256:…` once
//! are the bytes of `sha256:…` forever. Holding them is holding a fact about
//! content, and serving them again is exactly as correct as fetching them
//! again — which is what keeps a sweep inside a registry's pull quota,
//! since resolving a tag then costs a `HEAD` wherever the registry answers
//! one (ADR 0026 section 3).
//!
//! What is **never** held is an answer: that a tag is absent, which digest a
//! tag points at now, what a referrers list says, whether a digest exists in
//! a repository today. Every one of those changes while a component
//! publishes, and the crate's rule stands — nothing about what was *found*
//! is remembered between passes. So bytes held here are only ever used once
//! something asked fresh in the same call has said which digest is wanted:
//! a `HEAD` for a tag, or a manifest just read that names its config, its
//! children or its layer.
//!
//! # Bounded, oldest first
//!
//! At most [`MOST_ENTRIES`] entries and [`MOST_BYTES`] in all; inserting
//! past either evicts the oldest first. Bytes larger than the whole budget
//! are simply not held.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::client::digest::Content;

/// The most entries held.
pub(super) const MOST_ENTRIES: usize = 256;

/// The most bytes held, across every entry.
pub(super) const MOST_BYTES: usize = 8 * 1024 * 1024;

/// The cache.
#[derive(Debug, Default)]
pub(crate) struct Verified {
    /// What is held, behind a lock a poisoned holder cannot wedge: a
    /// poisoned cache is skipped, which costs a fetch and nothing else.
    held: Mutex<Held>,
}

/// What is held.
#[derive(Debug, Default)]
struct Held {
    /// Bytes by digest.
    by_digest: BTreeMap<String, Arc<[u8]>>,

    /// Digests, oldest first.
    order: VecDeque<String>,

    /// The sum of every entry's length.
    bytes: usize,
}

impl Verified {
    /// The bytes held for `digest`, if any.
    pub(super) fn get(&self, digest: &str) -> Option<Content> {
        let held = self.held.lock().ok()?;
        let bytes = held.by_digest.get(digest)?;

        Some(Content {
            digest: digest.to_owned(),
            bytes: Arc::clone(bytes),
        })
    }

    /// Holds `content`, evicting the oldest entries until it fits.
    ///
    /// Takes [`Content`], which only hashing produces, so nothing a registry
    /// merely claimed can be put here.
    pub(super) fn insert(&self, content: &Content) {
        let length = content.bytes.len();
        let Ok(mut held) = self.held.lock() else {
            return;
        };

        if length > MOST_BYTES || held.by_digest.contains_key(&content.digest) {
            return;
        }

        while held.by_digest.len() >= MOST_ENTRIES || held.bytes + length > MOST_BYTES {
            let Some(oldest) = held.order.pop_front() else {
                break;
            };
            if let Some(evicted) = held.by_digest.remove(&oldest) {
                held.bytes -= evicted.len();
            }
        }

        held.bytes += length;
        held.order.push_back(content.digest.clone());
        held.by_digest
            .insert(content.digest.clone(), Arc::clone(&content.bytes));
    }
}
