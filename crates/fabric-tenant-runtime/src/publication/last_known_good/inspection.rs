//! What an operator is shown about the copy, valid or not.

use super::envelope::Envelope;
use super::store::verify;
use super::LastKnownGoodStore;
use crate::publication::digest::sha256_hex;
use crate::publication::CatalogueCheck;

/// The copy as found on disk, for `fabric-api last-known-good inspect`.
///
/// Read-only: inspecting never writes, repairs or removes anything, so it
/// is safe against a copy the reader has refused.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Inspection {
    /// The file inspected.
    pub path: String,
    /// The SHA-256 of the whole file, to quote in an audit or an incident.
    pub file_sha256: Option<String>,
    /// The generation the copy's record states, read without trusting it:
    /// a refused copy still says which generation it claims to be.
    pub stated_generation: Option<u64>,
    /// Whether a reader would serve it.
    #[serde(flatten)]
    pub verdict: Verdict,
}

/// Whether a reader would serve the copy.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict {
    /// There is no copy.
    Absent,
    /// The copy checks against its own record and digests.
    Valid {
        /// Its generation.
        generation: u64,
        /// The SHA-256 of its record, which names the generation exactly.
        record_sha256: String,
    },
    /// The copy is refused.
    Refused {
        /// Why.
        reason: String,
    },
}

impl LastKnownGoodStore {
    /// Reads the copy and reports what it is and whether it would be served.
    ///
    /// `catalogue` is the host's own catalogue check, so a copy is reported
    /// valid only if the host could start from it.
    pub async fn inspect(&self, catalogue: &dyn CatalogueCheck) -> Inspection {
        let path = self.path().display().to_string();
        let bytes = match tokio::fs::read(self.path()).await {
            Ok(bytes) => bytes,
            Err(error) => {
                let verdict = if error.kind() == std::io::ErrorKind::NotFound {
                    Verdict::Absent
                } else {
                    Verdict::Refused {
                        reason: format!("could not read {path}: {error}"),
                    }
                };
                return Inspection {
                    path,
                    file_sha256: None,
                    stated_generation: None,
                    verdict,
                };
            }
        };
        let verdict = match verify(&bytes, catalogue) {
            Ok(generation) => Verdict::Valid {
                generation: generation.generation(),
                record_sha256: generation.complete.record_sha256(),
            },
            Err(refusal) => Verdict::Refused {
                reason: refusal.to_string(),
            },
        };
        Inspection {
            path,
            file_sha256: Some(sha256_hex(&bytes)),
            stated_generation: stated_generation(&bytes),
            verdict,
        }
    }
}

fn stated_generation(bytes: &[u8]) -> Option<u64> {
    #[derive(serde::Deserialize)]
    struct Stated {
        generation: u64,
    }
    let envelope: Envelope = serde_json::from_slice(bytes).ok()?;
    let stated: Stated = serde_json::from_str(&envelope.record).ok()?;
    Some(stated.generation)
}
