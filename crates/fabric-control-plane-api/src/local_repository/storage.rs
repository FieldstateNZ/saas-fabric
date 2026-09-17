//! Import and crash-safe replacement of a local snapshot.
use super::{unavailable, Record, Snapshot};
use fabric_client_model::ClientDocument;
use fabric_control_plane::RepositoryError;
use std::path::Path;
pub(super) async fn import(path: &Path) -> Result<Snapshot, String> {
    let mut snapshot = Snapshot::default();
    let mut entries = tokio::fs::read_dir(path).await.map_err(|e| e.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let file = entry.path();
        if file.extension().is_none_or(|extension| extension != "yaml") {
            continue;
        }
        let text = tokio::fs::read_to_string(&file)
            .await
            .map_err(|e| e.to_string())?;
        let document = ClientDocument::parse(&text).map_err(|e| e.to_string())?;
        let revision = snapshot.next_revision().map_err(|e| e.to_string())?;
        if snapshot
            .clients
            .insert(
                document.client().id.to_string(),
                Record {
                    revision: revision.to_string(),
                    text,
                },
            )
            .is_some()
        {
            return Err("Duplicate imported client".into());
        }
    }
    Ok(snapshot)
}
pub(super) async fn commit(path: &Path, snapshot: &Snapshot) -> Result<(), RepositoryError> {
    let bytes =
        serde_json::to_vec_pretty(snapshot).map_err(|_| unavailable("Could not serialize local state"))?;
    let temporary = path.with_extension("next");
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|_| unavailable("Could not write local state"))?;
    let file = tokio::fs::File::open(&temporary)
        .await
        .map_err(|_| unavailable("Could not open local state"))?;
    file.sync_all()
        .await
        .map_err(|_| unavailable("Could not flush local state"))?;
    tokio::fs::rename(&temporary, path)
        .await
        .map_err(|_| unavailable("Could not commit local state"))
}
