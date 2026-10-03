//! What a registry change's request carried, audited when it is refused
//! before the service sees it.
//!
//! # Why a refusal of the path or the body is audited too
//!
//! A host that is not one, a repository its grammar refuses — the Docker Hub
//! repository of one segment ADR 0026 names — or a body with a field this API
//! does not have is refused by an extractor, before any change takes its
//! turn. Every registry change is an audit event, refusals included (ADR 0026
//! section 5), and an operator probing what the rules accept is exactly what
//! an audit trail is for. So each change handler takes what it extracts as a
//! `Result`, and a refusal is audited here, with the operation it was for and
//! the host when the host itself was accepted. Nothing the request carried is
//! written: the refusal's stable code is the outcome.

use crate::audit::{registry_changed, RegistryOperation};
use crate::{ControlPlaneError, Operator, RegistryHost};

/// `extracted`, or its refusal audited as `operation` against `host`.
///
/// # Errors
///
/// The extractor's refusal, unchanged.
pub(super) fn admitted<T>(
    operator: &Operator,
    operation: RegistryOperation,
    host: Option<&RegistryHost>,
    extracted: Result<T, ControlPlaneError>,
) -> Result<T, ControlPlaneError> {
    extracted.inspect_err(|refusal| {
        registry_changed(operator.subject(), host, operation, refusal.code());
    })
}
