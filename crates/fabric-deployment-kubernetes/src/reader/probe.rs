//! Asking one pod what it serves, anonymously and within bounds.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use async_trait::async_trait;
use fabric_core::{ReaderGenerationReport, READER_GENERATION_MAX_BYTES, READER_GENERATION_PATH};
use fabric_platform_management::{ProbeFailure, ProbeOutcome};

/// The bound on one probe, connection to last byte.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[async_trait]
pub(crate) trait Probe: Send + Sync {
    async fn ask(&self, address: IpAddr) -> ProbeOutcome;
}

/// A client separate from the cluster client: no credential, no default
/// header, no proxy, no redirect.
pub(crate) struct HttpProbe {
    http: reqwest::Client,
    port: u16,
}

impl HttpProbe {
    pub(crate) fn new(port: u16) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(PROBE_TIMEOUT)
            .timeout(PROBE_TIMEOUT)
            .build()
            .map_err(|_| "could not build the reader probe client")?;
        Ok(Self { http, port })
    }
}

#[async_trait]
impl Probe for HttpProbe {
    async fn ask(&self, address: IpAddr) -> ProbeOutcome {
        let url = format!(
            "http://{}{READER_GENERATION_PATH}",
            SocketAddr::new(address, self.port)
        );
        let mut response = match self.http.get(url).send().await {
            Ok(response) => response,
            Err(error) => return ProbeOutcome::Failed(transport(&error)),
        };
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return ProbeOutcome::NothingServed;
        }
        if status.is_redirection() {
            return ProbeOutcome::Failed(ProbeFailure::Redirected);
        }
        if status != reqwest::StatusCode::OK {
            return ProbeOutcome::Failed(ProbeFailure::Status(status.as_u16()));
        }
        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) if body.len() + chunk.len() > READER_GENERATION_MAX_BYTES => {
                    return ProbeOutcome::Failed(ProbeFailure::TooLarge);
                }
                Ok(Some(chunk)) => body.extend_from_slice(&chunk),
                Ok(None) => break,
                Err(error) => return ProbeOutcome::Failed(transport(&error)),
            }
        }
        serde_json::from_slice::<ReaderGenerationReport>(&body)
            .ok()
            .and_then(|report| report.checked().ok())
            .map_or(
                ProbeOutcome::Failed(ProbeFailure::Malformed),
                ProbeOutcome::Reported,
            )
    }
}

fn transport(error: &reqwest::Error) -> ProbeFailure {
    if error.is_timeout() {
        ProbeFailure::TimedOut
    } else {
        ProbeFailure::Unreachable
    }
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
