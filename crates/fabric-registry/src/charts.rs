//! Reading a classic Helm chart repository.

mod index;
mod read;
mod transport;

use fabric_platform_management::{ChartIndex, RegistryError, Version};

use self::transport::Transport;

/// A chart repository, read over its published index.
///
/// # Anonymous, like the image registry
///
/// A chart repository serves `index.yaml` to anybody, so this carries no
/// credential — and a boundary that does not exist cannot be conflated with
/// one that does. The same reasoning as the OCI registry, for the same reason.
///
/// # HTTPS end to end
///
/// The index this reads names a version that gets pinned into what Argo
/// deploys, so a byte on the wire between here and the repository is a byte
/// that can steer a rollout. `reqwest`'s default redirect policy would follow
/// an `https://` request wherever a `30x` pointed it, including back down to
/// `http://` — making the first hop's HTTPS a formality rather than a
/// guarantee. The `transport` submodule enforces HTTPS on every hop instead,
/// initial request and every redirect alike.
pub struct HelmCharts {
    /// The HTTP client, built with a redirect policy matching `transport`.
    http: reqwest::Client,

    /// Which URLs this reader is permitted to speak to. `Copy`, so it can be
    /// captured by the redirect closure without a lifetime to carry.
    transport: Transport,
}

impl HelmCharts {
    /// Builds a reader that only ever speaks HTTPS.
    ///
    /// This is the only constructor the composition root calls. See
    /// [`plain_http_to_loopback`](Self::plain_http_to_loopback) for the one
    /// exception, which exists for tests alone.
    ///
    /// # Errors
    ///
    /// Returns a message if `http_timeout_seconds` is zero, or if the HTTP
    /// client cannot be built.
    pub fn new(http_timeout_seconds: u64) -> Result<Self, String> {
        Self::build(http_timeout_seconds, Transport::Https)
    }

    /// Builds a reader for a test that serves a chart index from a loopback
    /// socket.
    ///
    /// This crate's integration tests run a real HTTP server so they can
    /// check how this adapter behaves on the wire, and that server has no
    /// certificate to offer — plain HTTP is what a test can stand up without
    /// one. This constructor is the only way a [`HelmCharts`] accepts plain
    /// HTTP, it accepts that only to a loopback host, and every other rule is
    /// unchanged: a redirect off HTTPS, or off loopback, is still refused.
    /// Nothing in production ever calls this — [`new`](Self::new) is what the
    /// composition root uses, and it never widens past `Transport::Https`.
    ///
    /// # Errors
    ///
    /// The same as [`new`](Self::new).
    pub fn plain_http_to_loopback(http_timeout_seconds: u64) -> Result<Self, String> {
        Self::build(http_timeout_seconds, Transport::LoopbackToo)
    }

    /// Shared construction for both constructors.
    fn build(http_timeout_seconds: u64, transport: Transport) -> Result<Self, String> {
        if http_timeout_seconds == 0 {
            // reqwest reads zero as "no timeout", which is the difference
            // between a bounded discovery pass and one that hangs.
            return Err("chart repository: timeout_seconds is zero".to_owned());
        }

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(http_timeout_seconds))
            .user_agent("saas-fabric-control-plane")
            .redirect(transport::policy(transport))
            .build()
            .map_err(|error| format!("chart repository: {error}"))?;

        Ok(Self { http, transport })
    }
}

#[async_trait::async_trait]
impl ChartIndex for HelmCharts {
    async fn versions(&self, repository: &str, chart: &str) -> Result<Vec<Version>, RegistryError> {
        let raw_url = format!("{}/index.yaml", repository.trim_end_matches('/'));
        let url = transport::validated_index_url(self.transport, &raw_url)?;

        let body = read::bounded_get(&self.http, url).await?;

        index::versions_of(&body, chart)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_second_timeout_is_refused_by_both_constructors() {
        assert!(HelmCharts::new(0).is_err(), "new");
        assert!(
            HelmCharts::plain_http_to_loopback(0).is_err(),
            "plain_http_to_loopback"
        );
    }
}
