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

        let body = response
            .text()
            .await
            .map_err(|error| transport_failure("reading a chart index", &error))?;

        let index: Index = serde_norway::from_str(&body).map_err(|error| RegistryError::Refused {
            detail: format!("reading a chart index: {error}"),
        })?;

        Ok(index
            .entries
            .get(chart)
            .into_iter()
            .flatten()
            // A release the index lists whose version this cannot parse is
            // skipped rather than refused: a chart repository serves every
            // chart it holds, and one unparseable entry belonging to somebody
            // else's chart is not this component's problem.
            .filter_map(|entry| Version::parse(&entry.version))
            .collect())
    }
}
