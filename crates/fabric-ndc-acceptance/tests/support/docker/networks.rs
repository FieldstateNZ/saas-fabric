//! Docker network lifecycle: create, remove, and find leftovers by prefix.
//!
//! The "networks" concept the parent `docker.rs` module doc names. Builds on
//! [`process`](super::process) the same way [`containers`](super::containers)
//! does; the two files know nothing of each other.

use super::process;
use super::process::DockerError;

/// Creates a Docker network.
///
/// # Errors
///
/// A [`DockerError`] if `docker network create` failed.
pub fn network_create(name: &str) -> Result<(), DockerError> {
    process::run_checked(&["network".to_owned(), "create".to_owned(), name.to_owned()]).map(|_| ())
}

/// Removes a Docker network.
///
/// # Errors
///
/// A [`DockerError`] if `docker network rm` failed.
pub fn network_rm(name: &str) -> Result<(), DockerError> {
    process::run_checked(&["network".to_owned(), "rm".to_owned(), name.to_owned()]).map(|_| ())
}

/// Networks currently matching `prefix`: each one's name paired with
/// `docker network ls`'s own `{{.CreatedAt}}` reading for it -- see
/// [`super::containers::container_summaries_with_prefix`], the same shape
/// for the same reason.
///
/// # Errors
///
/// A [`DockerError`] if `docker network ls` failed.
pub fn network_summaries_with_prefix(prefix: &str) -> Result<Vec<(String, String)>, DockerError> {
    let output = process::run_checked(&[
        "network".to_owned(),
        "ls".to_owned(),
        "--filter".to_owned(),
        format!("name={prefix}"),
        "--format".to_owned(),
        "{{.Name}}\t{{.CreatedAt}}".to_owned(),
    ])?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, created_at)| (name.trim().to_owned(), created_at.trim().to_owned()))
        .collect())
}
