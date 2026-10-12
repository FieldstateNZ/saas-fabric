//! The production reader with a copy: a candidate is validated whole before
//! it can be chosen, raise the floor, or replace the copy; a refused copy
//! is kept until an operator discards it; and nothing is written unless the
//! copy is apart from every published file.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::fixture_files::{publish_into, unservable, write_copy};
use super::fixtures::{published, Catalogue};
use super::{DiscardRequest, LastKnownGoodState, LastKnownGoodStore};
use crate::publication::{GenerationOrigin, PublishedFiles, PublishedState, PublishedStateSource as _};
use crate::testing::TempDir;
use crate::SourceError;

struct Replica {
    _root: TempDir,
    published: PathBuf,
    copy: PathBuf,
}

impl Replica {
    fn new() -> Self {
        let root = TempDir::new("lkg-holder");
        let published = root.path().join("published");
        std::fs::create_dir_all(&published).unwrap();
        let copy = root.path().join("lkg");
        Self {
            published,
            copy,
            _root: root,
        }
    }

    fn store(&self) -> LastKnownGoodStore {
        LastKnownGoodStore::new(&self.copy)
    }

    /// A fresh reader over the same files: a restart.
    fn reader(&self) -> PublishedFiles {
        reader_over(&self.published, "tenants.json", &self.copy)
    }

    fn copy_bytes(&self) -> Vec<u8> {
        std::fs::read(self.store().path()).unwrap()
    }
}

fn reader_over(published: &Path, tenants: &str, copy: &Path) -> PublishedFiles {
    PublishedFiles::new(
        published.join(tenants),
        published.join("data-sources.json"),
        published.join("catalog.json"),
        Arc::new(Catalogue),
    )
    .with_last_known_good(LastKnownGoodStore::new(copy))
}

/// What the refresher does at start: load, and once a complete generation
/// is installed, say so, which is when the copy is persisted.
async fn start(reader: &PublishedFiles) -> Result<PublishedState, SourceError> {
    let state = reader.load_at_start().await?;
    install(reader, &state).await;
    Ok(state)
}

/// [`start`], on a refresh.
async fn refresh(reader: &PublishedFiles) -> Result<PublishedState, SourceError> {
    let state = reader.load().await?;
    install(reader, &state).await;
    Ok(state)
}

async fn install(reader: &PublishedFiles, state: &PublishedState) {
    if let PublishedState::Generation {
        generation,
        record_sha256,
        ..
    } = state
    {
        reader.installed(*generation, record_sha256).await;
    }
}

/// The fixture catalogue's own generation number, as the fixture decoder
/// decodes it.
fn catalogue_of(state: &PublishedState) -> u64 {
    let PublishedState::Generation { catalogue, .. } = state else {
        panic!("expected a complete generation, got {state:?}");
    };
    *catalogue.get::<u64>().unwrap()
}

fn generation_of(state: &PublishedState) -> (u64, GenerationOrigin) {
    let PublishedState::Generation {
        generation, origin, ..
    } = state
    else {
        panic!("expected a complete generation, got {state:?}");
    };
    (*generation, origin.clone())
}

fn is_held(origin: &GenerationOrigin) -> bool {
    matches!(origin, GenerationOrigin::LastKnownGood { .. })
}

async fn with_copy_of_generation_one() -> Replica {
    let replica = Replica::new();
    publish_into(&replica.published, &published(1, "pg-1"));
    start(&replica.reader()).await.unwrap();
    assert_eq!(replica.reader().last_known_good(), LastKnownGoodState::Absent);
    assert!(replica.store().path().exists());
    replica
}

#[tokio::test]
async fn at_start_an_unservable_higher_generation_holds_the_copy_and_leaves_its_bytes() {
    for (document, broken) in unservable(2) {
        let replica = with_copy_of_generation_one().await;
        let before = replica.copy_bytes();
        publish_into(&replica.published, &broken);

        let reader = replica.reader();
        let state = start(&reader).await.unwrap();

        let (generation, origin) = generation_of(&state);
        assert_eq!(generation, 1, "{document}");
        assert!(is_held(&origin), "{document}: {origin:?}");
        assert_eq!(catalogue_of(&state), 1, "{document}: the copy's own catalogue");
        assert_eq!(replica.copy_bytes(), before, "{document}: the copy must survive");
        assert_eq!(
            reader.last_known_good(),
            LastKnownGoodState::Stored { generation: 1 }
        );
    }
}

#[tokio::test]
async fn at_start_an_unservable_generation_with_no_copy_fails_closed_and_writes_nothing() {
    for (document, broken) in unservable(2) {
        let replica = Replica::new();
        publish_into(&replica.published, &broken);

        let refused = start(&replica.reader()).await.unwrap_err();

        assert!(
            matches!(&refused, SourceError::Unusable { document: named, .. } if *named == document),
            "{document}: {refused}"
        );
        assert!(!replica.store().path().exists(), "{document}");
    }
}

#[tokio::test]
async fn on_refresh_an_unservable_generation_is_held_without_raising_the_floor_or_touching_the_copy() {
    for (document, broken) in unservable(2) {
        let replica = Replica::new();
        publish_into(&replica.published, &published(1, "pg-1"));
        let reader = replica.reader();
        start(&reader).await.unwrap();
        let before = replica.copy_bytes();

        publish_into(&replica.published, &broken);
        let held = refresh(&reader).await.unwrap_err();
        assert!(matches!(held, SourceError::Unusable { .. }), "{document}: {held}");
        assert_eq!(replica.copy_bytes(), before, "{document}");

        publish_into(&replica.published, &published(1, "pg-1"));
        let state = refresh(&reader).await.unwrap();
        assert_eq!(generation_of(&state).0, 1, "{document}: the floor stayed at 1");
    }
}

#[tokio::test]
async fn after_a_held_refresh_a_restart_still_starts_from_the_copy_and_a_later_valid_generation_wins() {
    let replica = with_copy_of_generation_one().await;
    let (_, broken) = unservable(2).into_iter().next().unwrap();
    publish_into(&replica.published, &broken);
    assert!(refresh(&replica.reader()).await.is_err());

    let state = start(&replica.reader()).await.unwrap();
    let (generation, origin) = generation_of(&state);
    assert_eq!(generation, 1);
    assert!(is_held(&origin));

    publish_into(&replica.published, &published(3, "pg-1"));
    let reader = replica.reader();
    let state = start(&reader).await.unwrap();
    assert_eq!(generation_of(&state), (3, GenerationOrigin::Published));
    assert_eq!(
        reader.last_known_good(),
        LastKnownGoodState::Stored { generation: 3 }
    );
}

#[tokio::test]
async fn a_generation_loaded_but_never_installed_is_neither_persisted_nor_raises_the_floor() {
    let replica = with_copy_of_generation_one().await;
    let before = replica.copy_bytes();
    let reader = replica.reader();
    start(&reader).await.unwrap();

    publish_into(&replica.published, &published(2, "pg-1"));
    let loaded = reader.load().await.unwrap();
    assert_eq!(generation_of(&loaded).0, 2);
    assert_eq!(
        replica.copy_bytes(),
        before,
        "the refresher refused it: nothing persisted"
    );

    publish_into(&replica.published, &published(1, "pg-1"));
    let state = refresh(&reader).await.unwrap();
    assert_eq!(
        generation_of(&state),
        (1, GenerationOrigin::Published),
        "the floor stayed at 1"
    );
    assert_eq!(
        reader.last_known_good(),
        LastKnownGoodState::Stored { generation: 1 }
    );
}

#[tokio::test]
async fn a_refused_copy_is_kept_byte_for_byte_beside_a_valid_publication_until_it_is_discarded() {
    let replica = Replica::new();
    let (_, unservable_copy) = unservable(1).into_iter().next().unwrap();
    write_copy(replica.store().path(), &unservable_copy);
    let refused_bytes = replica.copy_bytes();
    publish_into(&replica.published, &published(2, "pg-1"));

    let reader = replica.reader();
    let state = start(&reader).await.unwrap();
    assert_eq!(generation_of(&state), (2, GenerationOrigin::Published));
    assert!(matches!(
        reader.last_known_good(),
        LastKnownGoodState::Refused { .. }
    ));
    assert_eq!(replica.copy_bytes(), refused_bytes);

    publish_into(&replica.published, &published(3, "pg-1"));
    refresh(&reader).await.unwrap();
    assert_eq!(
        replica.copy_bytes(),
        refused_bytes,
        "a refresh does not replace it either"
    );
    assert!(matches!(
        reader.last_known_good(),
        LastKnownGoodState::Refused { .. }
    ));

    let request = DiscardRequest {
        operator: "ops".to_owned(),
        reason: "refused copy reviewed".to_owned(),
    };
    replica
        .store()
        .discard(&request, 1, &Catalogue)
        .await
        .unwrap()
        .unwrap();
    publish_into(&replica.published, &published(4, "pg-1"));
    refresh(&reader).await.unwrap();
    assert_eq!(
        reader.last_known_good(),
        LastKnownGoodState::Stored { generation: 4 }
    );
}

#[tokio::test]
async fn a_copy_directory_aliased_onto_the_published_directory_is_never_written_into() {
    let replica = Replica::new();
    publish_into(&replica.published, &published(1, "pg-1"));
    let alias = replica.published.parent().unwrap().join("alias");
    symlink(&replica.published, &alias).unwrap();

    let reader = reader_over(&replica.published, "tenants.json", &alias);
    let state = start(&reader).await.unwrap();

    assert_eq!(generation_of(&state), (1, GenerationOrigin::Published));
    assert!(!replica.published.join("last-known-good.json").exists());
    assert!(
        matches!(reader.last_known_good(), LastKnownGoodState::PersistFailed { ref reason, .. } if reason.contains("apart")),
        "{:?}",
        reader.last_known_good()
    );
}

#[tokio::test]
async fn a_published_file_named_like_the_copy_under_an_aliased_directory_is_left_untouched() {
    let replica = Replica::new();
    let files = published(1, "pg-1");
    publish_into(&replica.published, &files);
    std::fs::rename(
        replica.published.join("tenants.json"),
        replica.published.join("last-known-good.json"),
    )
    .unwrap();
    let alias = replica.published.parent().unwrap().join("alias");
    symlink(&replica.published, &alias).unwrap();

    let reader = reader_over(&replica.published, "last-known-good.json", &alias);
    let _ = start(&reader).await;
    publish_into(&replica.published, &published(2, "pg-1"));
    std::fs::copy(
        replica.published.join("tenants.json"),
        replica.published.join("last-known-good.json"),
    )
    .unwrap();
    let _ = refresh(&reader).await;

    assert_eq!(
        std::fs::read(replica.published.join("last-known-good.json")).unwrap(),
        published(2, "pg-1").tenants,
        "the published tenants file is only ever what the publisher wrote"
    );
}
