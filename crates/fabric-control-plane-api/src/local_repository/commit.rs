//! Committing a mutated snapshot to disk before it becomes the current one,
//! without losing that commit to a cancelled caller.
use super::error_helpers::unavailable;
use super::{storage, Inner, Snapshot};
use fabric_control_plane::RepositoryError;
use std::sync::Arc;

/// Runs `mutate` against a clone of the current snapshot, commits the result
/// to disk, and — only once that succeeds — makes it the new current
/// snapshot.
///
/// Spawned as its own task, over `inner` rather than a borrow of the
/// repository, so the commit-and-swap runs to completion even if the caller
/// stops awaiting it — see
/// [`LocalClientRepository`](super::LocalClientRepository)'s own rustdoc for
/// why that matters.
pub(super) async fn commit_and_swap<T, F>(inner: Arc<Inner>, mutate: F) -> Result<T, RepositoryError>
where
    F: FnOnce(&mut Snapshot) -> Result<T, RepositoryError> + Send + 'static,
    T: Send + 'static,
{
    let task = tokio::spawn(async move {
        let mut stored = inner.state.lock().await;
        let mut next = stored.clone();
        let result = mutate(&mut next)?;
        storage::commit(&inner.path, &next).await?;
        *stored = next;
        Ok(result)
    });

    match task.await {
        Ok(result) => result,
        Err(_panicked) => Err(unavailable("the local repository write did not complete")),
    }
}
