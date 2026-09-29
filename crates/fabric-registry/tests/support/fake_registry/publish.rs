//! Publishing images and indexes, and the knobs that change how the fake
//! answers.

use serde_json::{json, Value};

use super::{path_of, sha256, FakeRegistry, Fixed, Head};

/// An OCI image manifest.
pub const OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";

/// An OCI index.
pub const OCI_INDEX: &str = "application/vnd.oci.image.index.v1+json";

/// The label and annotation a build stamps its commit into.
const REVISION: &str = "org.opencontainers.image.revision";

impl FakeRegistry {
    /// Stores a blob, returning its digest and size.
    pub fn put_blob(&self, repository: &str, body: &str) -> (String, usize) {
        let digest = sha256(body.as_bytes());
        self.locked()
            .blobs
            .insert((path_of(repository), digest.clone()), body.to_owned());
        (digest, body.len())
    }

    /// Stores a manifest, returning its digest.
    pub fn put_manifest(&self, repository: &str, body: &str) -> String {
        let digest = sha256(body.as_bytes());
        self.locked()
            .manifests
            .insert((path_of(repository), digest.clone()), body.to_owned());
        digest
    }

    /// Points a tag at a digest.
    pub fn tag(&self, repository: &str, tag: &str, digest: &str) {
        self.locked()
            .tags
            .insert((path_of(repository), tag.to_owned()), digest.to_owned());
    }

    /// Removes a tag, as a registry's garbage collection or a delete would.
    pub fn untag(&self, repository: &str, tag: &str) {
        self.locked().tags.remove(&(path_of(repository), tag.to_owned()));
    }

    /// Stores a single-platform image — a revision in its config's labels,
    /// its manifest's annotations, both or neither — and returns its digest.
    pub fn image(
        &self,
        repository: &str,
        label: Option<&str>,
        annotation: Option<&str>,
        marker: &str,
    ) -> String {
        let labels = label.map_or(Value::Null, |revision| json!({ REVISION: revision }));
        let config = json!({ "architecture": marker, "os": "linux", "config": { "Labels": labels } });
        let (config_digest, config_size) = self.put_blob(repository, &config.to_string());

        let mut manifest = json!({
            "schemaVersion": 2,
            "mediaType": OCI_MANIFEST,
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": config_digest,
                "size": config_size
            },
            "layers": []
        });
        if let Some(revision) = annotation {
            manifest["annotations"] = json!({ REVISION: revision });
        }
        self.put_manifest(repository, &manifest.to_string())
    }

    /// Publishes a single-architecture image carrying a revision label.
    pub fn publish(&self, repository: &str, tag: &str, revision: &str) {
        let digest = self.image(repository, Some(revision), None, "amd64");
        self.tag(repository, tag, &digest);
    }

    /// Publishes an image with no revision anywhere.
    pub fn publish_unlabelled(&self, repository: &str, tag: &str) {
        let digest = self.image(repository, None, None, "amd64");
        self.tag(repository, tag, &digest);
    }

    /// Publishes an image with a revision label, annotation, both or neither.
    pub fn publish_annotated(
        &self,
        repository: &str,
        tag: &str,
        label: Option<&str>,
        annotation: Option<&str>,
    ) {
        let digest = self.image(repository, label, annotation, "amd64");
        self.tag(repository, tag, &digest);
    }

    /// Publishes a multi-architecture index, one revision label per
    /// architecture, beside the attestation Buildx writes.
    pub fn publish_index(&self, repository: &str, tag: &str, children: &[(&str, &str)]) {
        let children: Vec<(&str, Option<&str>, Option<&str>)> = children
            .iter()
            .map(|(architecture, revision)| (*architecture, Some(*revision), None))
            .collect();
        self.publish_annotated_index(repository, tag, None, &children);
    }

    /// Publishes an index: its own annotation, and per child an architecture,
    /// a label and an annotation. An attestation entry is always added.
    pub fn publish_annotated_index(
        &self,
        repository: &str,
        tag: &str,
        annotation: Option<&str>,
        children: &[(&str, Option<&str>, Option<&str>)],
    ) {
        let mut entries: Vec<Value> = children
            .iter()
            .map(|(architecture, label, child_annotation)| {
                let digest = self.image(repository, *label, *child_annotation, architecture);
                json!({ "mediaType": OCI_MANIFEST, "digest": digest,
                        "platform": { "os": "linux", "architecture": architecture } })
            })
            .collect();
        entries.push(self.attestation(repository));

        let mut index = json!({ "schemaVersion": 2, "mediaType": OCI_INDEX, "manifests": entries });
        if let Some(revision) = annotation {
            index["annotations"] = json!({ REVISION: revision });
        }
        let digest = self.put_manifest(repository, &index.to_string());
        self.tag(repository, tag, &digest);
    }

    /// Publishes an index carrying nothing deployable at all: one
    /// attestation, which is what an interrupted push can leave behind.
    pub fn publish_index_with_no_image(&self, repository: &str, tag: &str) {
        let index = json!({ "schemaVersion": 2, "mediaType": OCI_INDEX,
                            "manifests": [self.attestation(repository)] });
        let digest = self.put_manifest(repository, &index.to_string());
        self.tag(repository, tag, &digest);
    }

    /// What Buildx puts in an index for a provenance attestation: an entry
    /// under `unknown/unknown`, with no revision.
    fn attestation(&self, repository: &str) -> Value {
        let digest = self.image(repository, None, None, "attestation");
        json!({ "mediaType": OCI_MANIFEST, "digest": digest,
                "platform": { "os": "unknown", "architecture": "unknown" } })
    }

    /// The digest the fake stored for a tag: read back from its state, so a
    /// test compares the adapter's answer with what was served.
    pub fn digest_for(&self, repository: &str, tag: &str) -> String {
        self.locked()
            .tags
            .get(&(path_of(repository), tag.to_owned()))
            .unwrap_or_else(|| panic!("{repository}:{tag} was never published"))
            .clone()
    }

    /// Answers every tag listing one tag at a time, with a `Link` header.
    pub fn paginate(&self) {
        self.locked().paginate = true;
    }

    /// Writes every `Link` target as an absolute URL under `origin`.
    pub fn link_under(&self, origin: &str) {
        self.locked().link_origin = Some(origin.to_owned());
    }

    /// Answers `tags/list` with a status instead of a listing.
    pub fn tags_answer(&self, status: u16) {
        self.locked().tags_status = Some(status);
    }

    /// Answers `HEAD` on a manifest as `head` says.
    pub fn head(&self, head: Head) {
        self.locked().head = head;
    }

    /// Answers every manifest with this `Docker-Content-Digest` instead.
    pub fn digest_header(&self, value: &str) {
        self.locked().digest_header = Some(value.to_owned());
    }

    /// Serves `digest`, a manifest or a blob, as bytes that do not hash to
    /// it.
    pub fn corrupt(&self, digest: &str) {
        self.locked().corrupt.insert(digest.to_owned());
    }

    /// Names a token response's bearer under `key` alone.
    pub fn token_key(&self, key: &str) {
        self.locked().token_key = Some(key.to_owned());
    }

    /// Answers `method` on every path starting with `path` with this reply.
    pub fn answer(
        &self,
        method: &str,
        path: &str,
        status: u16,
        headers: &[(&str, &str)],
        body: impl Into<String>,
    ) {
        self.locked().fixed.push(Fixed {
            method: method.to_owned(),
            path: path.to_owned(),
            status,
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
            body: body.into(),
        });
    }

    /// Refuses the token the fake minted last, as an aged-out one would be.
    pub fn expire_the_current_token(&self) {
        let mut state = self.locked();
        state.stale_token = Some(format!("token-{}", state.mints));
    }

    /// How many tokens have been minted.
    pub fn mints(&self) -> u64 {
        self.locked().mints
    }
}
