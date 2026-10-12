//! The copy's directory is refused unless it is physically apart from every
//! published file, however the two are named.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::check_separation;
use crate::testing::TempDir;

struct Layout {
    _root: TempDir,
    root: PathBuf,
    published: PathBuf,
}

fn layout() -> Layout {
    let root = TempDir::new("lkg-separation");
    let path = root.path().to_path_buf();
    let published = path.join("published");
    std::fs::create_dir_all(&published).unwrap();
    for name in [
        "tenants.json",
        "data-sources.json",
        "catalog.json",
        "publication.json",
    ] {
        std::fs::write(published.join(name), b"{}").unwrap();
    }
    Layout {
        root: path,
        published,
        _root: root,
    }
}

fn files(directory: &Path) -> Vec<PathBuf> {
    [
        "tenants.json",
        "data-sources.json",
        "catalog.json",
        "publication.json",
    ]
    .iter()
    .map(|name| directory.join(name))
    .collect()
}

fn check(directory: &Path, published: &[PathBuf]) -> Result<(), String> {
    let published: Vec<&Path> = published.iter().map(PathBuf::as_path).collect();
    check_separation(directory, &published)
}

#[test]
fn a_sibling_directory_is_apart_whether_or_not_it_exists_yet() {
    let layout = layout();
    assert!(check(&layout.root.join("lkg"), &files(&layout.published)).is_ok());
    assert!(check(&layout.root.join("not-yet/lkg"), &files(&layout.published)).is_ok());
}

#[test]
fn the_published_directory_itself_an_empty_path_and_a_nested_directory_are_refused() {
    let layout = layout();
    let published = files(&layout.published);
    assert!(check(&layout.published, &published).is_err());
    assert!(check(Path::new(""), &published).is_err());
    assert!(check(&layout.published.join("lkg"), &published).is_err());
    assert!(
        check(&layout.root, &published).is_err(),
        "a parent of the published files"
    );
}

#[test]
fn a_symlink_alias_of_the_published_directory_is_refused() {
    let layout = layout();
    let alias = layout.root.join("alias");
    symlink(&layout.published, &alias).unwrap();

    assert!(check(&alias, &files(&layout.published)).is_err());
    assert!(check(&alias.join("not-yet"), &files(&layout.published)).is_err());
}

#[test]
fn a_dot_dot_alias_is_refused_even_through_a_directory_that_does_not_exist() {
    let layout = layout();
    let dotted = layout.root.join("lkg/../published");
    let through_missing = layout.root.join("missing/../published/./");

    assert!(check(&dotted, &files(&layout.published)).is_err());
    assert!(check(&through_missing, &files(&layout.published)).is_err());
}

#[test]
fn a_dot_dot_is_taken_after_the_symlink_before_it_as_the_kernel_takes_it() {
    let layout = layout();
    let deep = layout.root.join("a/b");
    std::fs::create_dir_all(&deep).unwrap();
    let link = layout.root.join("link");
    symlink(&deep, &link).unwrap();
    // link/.. is a/, not the root: lexically it would look like the root.
    let published_in_a = layout.root.join("a/published");
    std::fs::create_dir_all(&published_in_a).unwrap();

    assert!(check(&link.join("../published"), &files(&published_in_a)).is_err());
    assert!(check(&link.join("../lkg"), &files(&published_in_a)).is_ok());
}

#[test]
fn a_published_file_with_the_copys_own_name_in_the_copys_directory_is_refused() {
    let layout = layout();
    let custom = vec![layout.published.join("last-known-good.json")];
    let alias = layout.root.join("alias");
    symlink(&layout.published, &alias).unwrap();

    assert!(check(&alias, &custom).is_err());
}

#[test]
fn a_published_file_that_is_a_symlink_into_the_copys_directory_is_refused() {
    let layout = layout();
    let copy_directory = layout.root.join("lkg");
    std::fs::create_dir_all(&copy_directory).unwrap();
    let mounted = layout.root.join("mounted");
    std::fs::create_dir_all(&mounted).unwrap();
    symlink(copy_directory.join("tenants.json"), mounted.join("tenants.json")).unwrap();
    std::fs::write(copy_directory.join("tenants.json"), b"{}").unwrap();

    assert!(check(&copy_directory, &[mounted.join("tenants.json")]).is_err());
}

#[test]
fn a_kubernetes_style_mount_whose_files_link_into_a_hidden_data_directory_is_refused_as_the_copy() {
    let layout = layout();
    let mount = layout.root.join("etc-fabric");
    let data = mount.join("..2026_10_12");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("tenants.json"), b"[]").unwrap();
    symlink(&data, mount.join("..data")).unwrap();
    symlink(Path::new("..data/tenants.json"), mount.join("tenants.json")).unwrap();

    assert!(check(&mount, &[mount.join("tenants.json")]).is_err());
    assert!(check(&layout.root.join("lkg"), &[mount.join("tenants.json")]).is_ok());
}
