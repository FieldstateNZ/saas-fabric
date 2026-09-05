//! What one document's publication must do, decided without touching a
//! filesystem.
//!
//! This is exactly ADR 0018's two tables (part 6), reproduced as a pure
//! function so every row is a unit test rather than an assertion in a doc
//! comment. Held state and incoming state are both plain data — an
//! adapter's job is to read them off disk (or a `ConfigMap`, for a future
//! Kubernetes adapter) and hand them here; this function never performs I/O
//! itself.

use crate::{DocumentKind, DocumentRevision, PublicationError};

/// What must happen to one document, once its verdict is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The payload — and, when it changes, the manifest — must be written.
    Write,
    /// Nothing changed. Nothing is written, not even the manifest.
    Unchanged,
}

/// What is currently held for one document, read from its manifest and
/// payload before any write is attempted.
///
/// `None` at the call site (rather than a value of this type) is the
/// presence table's "no manifest held" row: first publication, write,
/// divergence guard off, regardless of any payload bytes that might
/// nonetheless be sitting on disk.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Held<'a> {
    /// The revision recorded in the held manifest.
    pub(crate) revision: DocumentRevision,
    /// The held payload's raw bytes, if the payload file exists. `None`
    /// here is the presence table's "manifest held, payload absent" row —
    /// there is nothing to diverge from, so a publication at the held
    /// revision is a republication, not a divergence.
    pub(crate) payload: Option<&'a [u8]>,
}

/// One document being offered to [`crate::RuntimePublication::publish`], as
/// far as the verdict needs to know.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Incoming<'a> {
    /// Which document this is, so a refusal can name it.
    pub(crate) document: DocumentKind,
    /// The revision the caller asserts this publication moves the document
    /// to.
    pub(crate) revision: DocumentRevision,
    /// The document's canonical serialised bytes.
    pub(crate) payload: &'a [u8],
}

/// Decides what one document's publication must do.
///
/// # Errors
///
/// Returns [`PublicationError::StaleRevision`] if `incoming` is older than
/// what is held, or [`PublicationError::DivergentPayload`] if it repeats the
/// held revision with different bytes. Both refuse the whole publication —
/// see the crate's module docs.
pub(crate) fn verdict(held: Option<Held<'_>>, incoming: &Incoming<'_>) -> Result<Verdict, PublicationError> {
    let Some(held) = held else {
        // No manifest held at all: first publication. Write, unconditionally
        // — even an orphaned payload file left over with no manifest beside
        // it (the shipped `examples/*.json` today) does not gate this.
        return Ok(Verdict::Write);
    };

    let Some(held_payload) = held.payload else {
        // Manifest held, payload missing: nothing to diverge from, so this
        // is a republication rather than a divergence.
        return Ok(Verdict::Write);
    };

    if incoming.revision < held.revision {
        return Err(PublicationError::StaleRevision {
            document: incoming.document,
            held: held.revision,
            offered: incoming.revision,
        });
    }

    if incoming.revision > held.revision {
        return Ok(Verdict::Write);
    }

    if held_payload == incoming.payload {
        Ok(Verdict::Unchanged)
    } else {
        Err(PublicationError::DivergentPayload {
            document: incoming.document,
            revision: incoming.revision,
        })
    }
}
