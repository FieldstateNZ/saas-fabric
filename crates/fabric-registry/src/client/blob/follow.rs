//! Following a blob's redirects by hand, so the pull token goes to the
//! registry's own origin and nowhere else.
//!
//! # Why by hand, and not through `reqwest`'s redirect policy
//!
//! `reqwest` follows redirects through `tower-http`'s `FollowRedirect`, which
//! rebuilds every hop from the *first* request's headers and then lets
//! `reqwest` strip `Authorization` only when the hop changes host or port
//! from the hop *before it*. On a chain registry → CDN → the same CDN, the
//! last hop is "the same host as the previous one", so it gets the
//! registry's `Authorization` back. Deciding here, per hop, against the
//! registry's own origin rather than the previous hop keeps ADR 0026
//! section 4's guarantee — credentials never follow a redirect to another
//! origin — for chains of any length. The pull token is anonymous today;
//! the same path will carry an operator's registry credential.

use reqwest::header::{ACCEPT, LOCATION};
use reqwest::{Method, Response, StatusCode};

use fabric_platform_management::RegistryError;

use crate::client::http::refusal;
use crate::client::send::Via;
use crate::client::OciRegistry;
use crate::errors::send_failure;
use crate::transport::{permits, same_origin};

/// What every blob request asks for.
const ANY: &str = "*/*";

impl OciRegistry {
    /// `GET` a blob, following at most
    /// [`MAX_REDIRECTS`](crate::transport::MAX_REDIRECTS) redirects to any
    /// origin the transport rule permits, with the pull token on the
    /// registry's own origin only. The answer is the last hop's response.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if a request could not be sent, if the token could
    /// not be minted, or if a redirect had no usable `Location`, left HTTPS,
    /// passed the bound or named a credential. No refusal names the target:
    /// a CDN's signed query is a credential.
    pub(super) async fn follow_blob(
        &self,
        repository: &str,
        url: &str,
        operation: &str,
    ) -> Result<Response, RegistryError> {
        let mut response = self
            .send(Method::GET, Via::Blob, operation, repository, url, ANY)
            .await?;
        let mut hops = Vec::new();

        while is_redirect(response.status()) {
            let current = response.url().clone();
            let next = response
                .headers()
                .get(LOCATION)
                .and_then(|location| location.to_str().ok())
                .and_then(|location| current.join(location).ok())
                .ok_or_else(|| RegistryError::Refused {
                    detail: format!("{operation} was redirected without a location it could follow"),
                })?;
            hops.push(current);

            permits(self.transport, &hops, &next).map_err(|refused| RegistryError::Refused {
                detail: refusal(refused),
            })?;
            if carries_credential(&next) {
                return Err(RegistryError::Refused {
                    detail: format!("{operation} was redirected to a location carrying a credential"),
                });
            }

            let mut request = self.blobs.request(Method::GET, next.clone()).header(ACCEPT, ANY);
            if same_origin(&next, &self.origin) {
                request = request.bearer_auth(self.token(operation, repository, false).await?);
            }
            response = request
                .send()
                .await
                .map_err(|error| send_failure(operation, &error))?;
        }

        Ok(response)
    }
}

/// Whether a redirect target names a user or a password in its authority.
///
/// `reqwest` turns a URL's `user:password@` into a `Basic` `Authorization`
/// header on the request it builds from it, so following such a target
/// would send a credential the far end chose, to a host the far end chose —
/// on a hop this crate otherwise sends anonymously. `link.rs` refuses the
/// same shape in a `Link`. Checked before the hop is sent, and worded like
/// every other
/// refusal here: without the target, and without what it carried.
fn carries_credential(url: &reqwest::Url) -> bool {
    !url.username().is_empty() || url.password().is_some()
}

/// Whether a status is one a `GET` follows: `301`, `302`, `303`, `307` or
/// `308`.
fn is_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}
