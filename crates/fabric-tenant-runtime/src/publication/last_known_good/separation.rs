//! Keeping the copy's directory physically apart from every published file.
//!
//! A lexical comparison is not enough: a symlink, a `..` segment or a
//! published file that is itself a symlink (as every file in a Kubernetes
//! `ConfigMap` mount is) can name the same directory two ways. Each path is
//! therefore resolved through the filesystem as far as it exists, and the
//! rest, which cannot contain a symlink yet, is normalised lexically.

use std::io;
use std::path::{Component, Path, PathBuf};

/// Refuses a last-known-good `directory` that is empty, or that is, holds,
/// or sits inside a directory holding any of the `published` files, once
/// every symlink and `..` is resolved.
///
/// Both the configuration check and the reader, before every write, call
/// this, so a directory that became an alias after start is still refused
/// before anything is written into it.
///
/// # Errors
///
/// A message naming the directory and the published file it collides with.
pub fn check_separation(directory: &Path, published: &[&Path]) -> Result<(), String> {
    if directory.as_os_str().is_empty() {
        return Err("the last-known-good directory must not be empty".to_owned());
    }
    let copy_directory = resolve(directory).map_err(|error| describe(directory, &error))?;
    for file in published {
        let resolved_file = resolve(file).map_err(|error| describe(file, &error))?;
        let named_parent = file
            .parent()
            .map(resolve)
            .transpose()
            .map_err(|error| describe(file, &error))?;
        let parents = [resolved_file.parent().map(Path::to_path_buf), named_parent];
        for parent in parents.into_iter().flatten() {
            if parent.starts_with(&copy_directory) || copy_directory.starts_with(&parent) {
                return Err(format!(
                    "the last-known-good directory {} resolves to {}, which is not apart from the published \
                     file {} (in {}): the reader never writes where the publisher does",
                    directory.display(),
                    copy_directory.display(),
                    file.display(),
                    parent.display()
                ));
            }
        }
    }
    Ok(())
}

/// `path` resolved component by component: each prefix that exists is
/// resolved through the filesystem (following symlinks) before the next
/// component, and `..` steps back from what was resolved, as the kernel
/// would. A prefix that does not exist yet cannot be a symlink, so the rest
/// is joined as written.
fn resolve(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                resolved.push(name);
                match resolved.canonicalize() {
                    Ok(real) => resolved = real,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
            Component::RootDir | Component::Prefix(_) => resolved.push(component),
        }
    }
    Ok(resolved)
}

fn describe(path: &Path, error: &io::Error) -> String {
    format!("could not resolve {}: {error}", path.display())
}
