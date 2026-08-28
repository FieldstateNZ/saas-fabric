//! What a caller is allowed to be told, and how it is sent.

use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_tenant_runtime::ResolveError;
use http::header::RETRY_AFTER;
use http::HeaderValue;

use crate::errors::connector_mapping::{self, PublicMessage};
use crate::{logging, request_id, DataApiError};

impl DataApiError {
    /// The message the caller sees.
    ///
    /// Internal failures collapse to a fixed string. The detail is not lost —
    /// it goes to the log with the trace id — but it does not travel to an
    /// application that has no use for it and should not learn it.
    ///
    /// *Which* fixed string is not always "internal error". A failure raised
    /// while a mutation was in flight says what is known about whether the
    /// mutation happened, because a caller who guesses that wrong either
    /// duplicates a write or loses one. `connector_mapping` decides.
    pub(crate) fn public_message(&self) -> String {
        match self {
            Self::Resolve(ResolveError::RuntimeUnavailable) => {
                "the platform is starting up; retry shortly".to_owned()
            }
            Self::Resolve(ResolveError::UnknownTenant(_)) => "this tenant has no resources here".to_owned(),

            // All three name platform topology — a logical data source the
            // tenant never declared, a DataSource id, or a DataSource id plus
            // the isolation model it was asked for. None of that is an
            // application's business (§2), and the isolation one is the
            // sharpest of the three: it would tell a caller both which
            // physical resource backs them and that its tenant boundary is
            // currently misconfigured.
            Self::Resolve(
                ResolveError::UnboundDataSource { .. }
                | ResolveError::MissingDataSource { .. }
                | ResolveError::IsolationNotEnforceable { .. },
            ) => "internal error".to_owned(),

            // Connector text can name tables, schemas and servers, so what a
            // caller is told is decided by the same table that decides the
            // status — see `errors::connector_mapping`. Two things it settles
            // that are easy to get wrong: a write whose outcome is unknown must
            // not be told the request failed, and a write that provably landed
            // must not be told to retry.
            Self::Connector { error, operation } => {
                match connector_mapping::answer(error, *operation).message {
                    PublicMessage::Fixed(message) => message.to_owned(),

                    // The one message that repeats anything a connector said —
                    // and it repeats a `&'static str` from
                    // `UnsupportedFeature`'s closed set, so there are no
                    // connector-supplied bytes here to leak. This used to be an
                    // allowlist in this crate, because `feature` was a `String`
                    // and the producing side could not be trusted with it; the
                    // type carries that guarantee now, and the allowlist was
                    // deleted rather than left looking load-bearing.
                    //
                    // The refusal's `detail` — the collection, field, or
                    // procedure it was raised over — is not readable from here
                    // even by mistake: `RefusalDetail` has no `Display`. It is
                    // recorded below.
                    PublicMessage::Unsupported(feature) => {
                        format!("this operation is not supported: {feature}")
                    }
                }
            }

            other => other.to_string(),
        }
    }
}

impl IntoResponse for DataApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let id = request_id::current();

        match &self {
            // §28's anti-enumeration measure: externally this looks exactly
            // like any other unknown tenant. Internally it is its own event,
            // so an operator can tell probing apart from a reconciliation
            // failure without either changing what the caller sees.
            Self::Resolve(ResolveError::UnknownTenant(tenant)) => {
                logging::unknown_tenant_probed(tenant, &id);
            }
            // Everything else that reaches the caller as a 5xx is logged
            // here, rather than at the throw site, so there is exactly one
            // place every masked internal detail is recorded.
            _ if status.is_server_error() => {
                logging::request_failed(self.code(), &self.to_string(), &id);
            }
            // Connector failures that answer 4xx, which the arm above misses.
            // Correct-for-disclosure and, until now, invisible: an
            // `InvalidOperation` or `UnknownCollection` is a catalogue
            // misconfiguration whose detail is replaced on the way out and was
            // recorded nowhere, so the only trace of a mis-catalogued resource
            // was the caller's 400.
            //
            // `operator_message`, not `to_string`: a refusal's physical
            // specifics are held out of `Display` precisely so nothing can
            // forward them by accident, which makes asking for them here the
            // only way they are ever written down.
            //
            // Below the 5xx arm, so a connector error that is a 5xx keeps its
            // existing event and is never recorded twice.
            Self::Connector { error, .. } => {
                logging::connector_refused(self.code(), &error.operator_message(), &id);
            }
            _ => {}
        }

        let body = serde_json::json!({
            "error": {
                "code": self.code(),
                "message": self.public_message(),
                // Safe to return unconditionally: it names nothing but this
                // one request, and is either the caller's own header value
                // echoed back or an id generated fresh for them.
                "request_id": id,
            }
        });

        let mut response = (status, Json(body)).into_response();

        // Only where the platform is genuinely inviting a retry. Its absence is
        // as load-bearing as its presence: a write whose outcome is unknown
        // must not carry one, because the platform cannot make that retry safe.
        if let Some(seconds) = self.retry_after() {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from(seconds));
        }

        response
    }
}
