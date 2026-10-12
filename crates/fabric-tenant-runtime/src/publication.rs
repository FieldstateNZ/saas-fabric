//! Reading the published tenants and data-sources documents as one
//! generation.
//!
//! The publisher writes data sources, then the catalogue, then tenants, and
//! last a publication record naming the exact bytes of each. A reader that
//! finds both documents matching the record holds one complete generation;
//! one that finds either differing is mid-publication, after an interrupted
//! one, or ahead on one Kubernetes volume, and keeps the generation it
//! already serves. That is what makes a publication all-or-nothing to a
//! reader, though the three documents are written one at a time (gaps G1b,
//! G2 and G3 in `docs/roadmap/m2-publication-gap-report.md`).
//!
//! The record shape here is this plane's own copy, as for every published
//! document: the runtime plane may not depend on the publisher's crate.

mod catalogue;
mod catalogue_check;
mod complete_generation;
mod digest;
mod file_reads;
mod last_known_good;
mod parsed_generation;
mod published_files;
#[cfg(test)]
mod published_files_tests;
mod published_state;
mod reader_status;
mod record;

pub use catalogue::startup_catalogue;
pub use catalogue_check::CatalogueCheck;
pub use last_known_good::{
    check_separation, DiscardRequest, Discarded, Inspection, LastKnownGoodRefusal, LastKnownGoodState,
    LastKnownGoodStore, Verdict, AUDIT_FILE_NAME,
};
pub use published_files::PublishedFiles;
pub use published_state::{GenerationOrigin, PublishedState, PublishedStateSource};
pub use reader_status::{ReaderReport, ReaderStatus, Serving};
pub use record::READER_CONTRACT;
