//! Reading a classic Helm chart repository.

use fabric_platform_management::{ChartIndex, RegistryError, Version};

use crate::errors::{status_failure, transport_failure};

/// A chart repository, read over its published index.
///
/// # Anonymous, like the image registry
///
/// A chart repository serves `index.yaml` to anybody, so this carries no
/// credential — and a boundary that does not exist cannot be conflated with
/// one that does. The same reasoning as the OCI registry, for the same reason.
pub struct HelmCharts {
    /// The HTTP client.
    http: reqwest::Client,
}

/// How much of an index this will read.
///
/// A chart repository serves every chart it holds in one document, and a busy
/// one is large: the index this platform reads today is around 200 kB for 220
/// releases. Eight megabytes is far past any of that and still a bound — an
/// unbounded read of a remote document is an unbounded allocation decided by
/// somebody else.
const MOST: usize = 8 * 1024 * 1024;

/// Just enough of an index to list versions.
///
/// A chart index carries a great deal this does not read — descriptions,
/// digests of the packaged archive, maintainers, `appVersion`. Naming only
/// what is used keeps a field this platform does not act on from looking like
/// one it does.
#[derive(serde::Deserialize)]
struct Index {
    /// Releases by chart name.
    #[serde(default)]
    entries: std::collections::BTreeMap<String, Vec<Entry>>,
}

/// One published release of a chart.
#[derive(serde::Deserialize)]
struct Entry {
    /// The chart version, which is what Argo pins.
    version: String,
}

impl HelmCharts {
    /// Builds a reader.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be built.
    pub fn new(http_timeout_seconds: u64) -> Result<Self, String> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(http_timeout_seconds))
                .user_agent("saas-fabric-control-plane")
                .build()
                .map_err(|error| format!("chart repository: {error}"))?,
        })
    }
}

#[async_trait::async_trait]
impl ChartIndex for HelmCharts {
    async fn versions(&self, repository: &str, chart: &str) -> Result<Vec<Version>, RegistryError> {
        let url = format!("{}/index.yaml", repository.trim_end_matches('/'));

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|error| transport_failure("reading a chart index", &error))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(status_failure(
                "reading a chart index",
                status,
                response.headers(),
            ));
        }

        // Streamed rather than `text()`, so the bound is applied as the body
        // arrives instead of after it has all been held.
        let mut body = Vec::new();
        let mut stream = response;

        while let Some(chunk) = stream
            .chunk()
            .await
            .map_err(|error| transport_failure("reading a chart index", &error))?
        {
            if body.len() + chunk.len() > MOST {
                return Err(RegistryError::Refused {
                    detail: format!("the chart index at {url} is larger than {MOST} bytes"),
                });
            }
            body.extend_from_slice(&chunk);
        }

        let body = String::from_utf8(body).map_err(|_| RegistryError::Refused {
            detail: format!("the chart index at {url} is not text"),
        })?;

        let index: Index = serde_norway::from_str(&body).map_err(|error| RegistryError::Refused {
            detail: format!("reading a chart index: {error}"),
        })?;

        // Only this chart's own releases. Another chart's malformed entry is
        // not this component's problem; one of its own is.
        let Some(releases) = index.entries.get(chart) else {
            return Ok(Vec::new());
        };

        let mut versions = Vec::with_capacity(releases.len());

        for entry in releases {
            // Refused, not skipped. A version this cannot read is one it
            // cannot order either, so skipping it would mean answering "the
            // newest is X" while holding something that might have been newer
            // -- a wrong answer given confidently, which is worse than none.
            let version = Version::parse_chart(&entry.version).ok_or_else(|| RegistryError::Refused {
                detail: format!("{chart} lists '{}', which is not a version", entry.version),
            })?;

            // Two releases of equal precedence -- the same version twice, or
            // two differing only in build metadata, which SemVer says is not a
            // difference. There is no newest of them, and picking would be
            // picking arbitrarily.
            if versions.contains(&version) {
                return Err(RegistryError::Refused {
                    detail: format!("{chart} lists {version} more than once"),
                });
            }

            versions.push(version);
        }

        Ok(versions)
    }
}
