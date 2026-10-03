//! Attaching artifacts to an image, as ORAS does: through the referrers API
//! where the registry serves it, and through the referrers tag schema where
//! it does not.

use serde_json::{json, Value};

use super::publish::{OCI_INDEX, OCI_MANIFEST};
use super::{path_of, FakeRegistry};

/// A v1 component descriptor's artifact type.
pub const COMPONENT: &str = "application/vnd.saas-fabric.component.v1";

/// Which listing an attached artifact appears in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Listed {
    /// The referrers API's, which the registry maintains.
    Api,

    /// The referrers tag schema's index, which the publisher maintains.
    TagSchema,

    /// Both, as after a registry started serving the API.
    Both,
}

impl FakeRegistry {
    /// Serves the referrers API, as Docker Hub does. Without this the fake
    /// answers it `404 MANIFEST_UNKNOWN`, as GHCR does.
    pub fn serve_referrers_api(&self) {
        self.locked().serves_referrers = true;
    }

    /// A component descriptor's manifest for `document`, attached to
    /// `subject`, with its layer stored — for a test to change before
    /// [`attach_raw`](Self::attach_raw).
    pub fn descriptor_manifest(&self, repository: &str, subject: &str, document: &str) -> Value {
        let (empty, _) = self.put_blob(repository, "{}");
        let (layer, size) = self.put_blob(repository, document);
        json!({
            "schemaVersion": 2,
            "mediaType": OCI_MANIFEST,
            "artifactType": COMPONENT,
            "config": { "mediaType": "application/vnd.oci.empty.v1+json", "digest": empty, "size": 2 },
            "layers": [{ "mediaType": format!("{COMPONENT}+json"), "digest": layer, "size": size }],
            "subject": { "mediaType": OCI_MANIFEST, "digest": subject, "size": 1 },
            "annotations": {
                "org.opencontainers.image.revision": "5320432",
                "org.opencontainers.image.version": "1.4.0"
            }
        })
    }

    /// Attaches `document` as a component descriptor to the image `tag`
    /// points at, returning the descriptor manifest's digest.
    pub fn attach(&self, repository: &str, tag: &str, document: &str, listed: Listed) -> String {
        let subject = self.digest_for(repository, tag);
        let manifest = self.descriptor_manifest(repository, &subject, document);
        self.attach_raw(repository, &subject, &manifest, listed)
    }

    /// Stores `manifest` and lists it as a referrer of `subject`.
    pub fn attach_raw(&self, repository: &str, subject: &str, manifest: &Value, listed: Listed) -> String {
        let body = manifest.to_string();
        let digest = self.put_manifest(repository, &body);
        let artifact_type = manifest["artifactType"].as_str().unwrap_or_default().to_owned();
        self.list(repository, subject, &digest, &artifact_type, body.len(), listed);
        digest
    }

    /// Lists `digest` as a referrer of `subject`, whether or not it is stored.
    pub fn list(
        &self,
        repository: &str,
        subject: &str,
        digest: &str,
        artifact_type: &str,
        size: usize,
        listed: Listed,
    ) {
        let entry = json!({ "mediaType": OCI_MANIFEST, "digest": digest, "size": size,
                            "artifactType": artifact_type });
        let key = (path_of(repository), subject.to_owned());

        if listed != Listed::TagSchema {
            self.locked()
                .referrers
                .entry(key.clone())
                .or_default()
                .push(entry.clone());
        }
        if listed != Listed::Api {
            let entries = {
                let mut state = self.locked();
                let entries = state.tag_schema.entry(key).or_default();
                entries.push(entry);
                entries.clone()
            };
            let index = json!({ "schemaVersion": 2, "mediaType": OCI_INDEX, "manifests": entries });
            self.tag_schema_holds(repository, subject, &index.to_string());
        }
    }

    /// Stores `body` under the referrers tag `sha256-<hex>` for `subject`.
    pub fn tag_schema_holds(&self, repository: &str, subject: &str, body: &str) {
        let digest = self.put_manifest(repository, body);
        self.tag(repository, &subject.replacen(':', "-", 1), &digest);
    }
}
