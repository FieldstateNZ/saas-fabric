//! Loading development desired state from a directory.

use std::path::Path;

use fabric_client_model::ClientDocument;
use fabric_control_plane::InMemoryClientRepository;

/// Reads every `*.yaml` document under `path` into the repository.
///
/// # Every document must parse
///
/// A file that will not parse fails startup rather than being skipped. This is
/// a development adapter, so the cost of being strict is a clear error at
/// `cargo run` — and the cost of being lenient is a client that silently is
/// not there, which is the confusing failure this whole increment tries to
/// avoid at every layer.
///
/// # Errors
///
/// Returns a message if the directory cannot be read, if a document will not
/// parse, or if the directory holds no documents at all.
pub(super) async fn load(repository: &InMemoryClientRepository, path: &Path) -> Result<usize, String> {
    let mut entries = tokio::fs::read_dir(path)
        .await
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;

    let mut loaded = 0;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| format!("could not read {}: {error}", path.display()))?
    {
        let file = entry.path();
        if file.extension().is_none_or(|extension| extension != "yaml") {
            continue;
        }

        let text = tokio::fs::read_to_string(&file)
            .await
            .map_err(|error| format!("could not read {}: {error}", file.display()))?;

        let document = ClientDocument::parse(&text)
            .map_err(|error| format!("{} is not a client document: {error}", file.display()))?;

        repository
            .insert(document)
            .map_err(|error| format!("could not load {}: {error}", file.display()))?;
        loaded += 1;
    }

    if loaded == 0 {
        return Err(format!("{} holds no client documents", path.display()));
    }

    Ok(loaded)
}
