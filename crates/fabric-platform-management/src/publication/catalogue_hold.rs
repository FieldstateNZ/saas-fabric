//! Keeping the last published catalogue while the derived one names a
//! logical data source no tenant binds yet (G8).

use fabric_runtime_publication::{
    unbound_catalogue_entries, DocumentInput, PublishedResources, PublishedRevisions, RuntimeSnapshot,
};

/// Offers the held catalogue, at its held revision, in place of the derived
/// one whenever the derived one has an entry no composed tenant binds and a
/// catalogue is held.
///
/// The publisher refuses such a catalogue outright. Refusing the pass would
/// also hold back every tenant and data-source change behind a release
/// that nobody is placed on yet, so the catalogue alone waits at its last
/// complete generation, and is offered again on the next pass. With
/// nothing held there is no complete generation to keep, and the offer goes
/// ahead for the publisher to refuse.
pub(super) fn hold_unbound_catalogue(
    snapshot: &mut RuntimeSnapshot,
    published: &PublishedResources,
    held: &PublishedRevisions,
) {
    let unbound = unbound_catalogue_entries(&snapshot.catalog.payload, &snapshot.tenants.payload);
    let (Some((resource, data_source)), Some(catalog), Some(revision)) =
        (unbound.first(), published.catalog.as_ref(), held.catalog)
    else {
        return;
    };
    tracing::warn!(
        %resource,
        %data_source,
        unbound = unbound.len(),
        "the derived catalogue names a logical data source no tenant binds; keeping the published catalogue"
    );
    snapshot.catalog = DocumentInput::new(revision, catalog.clone());
}
