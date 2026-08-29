//! Describing the application this platform wants, and where to approve it.

use serde_json::{json, Value};

use crate::provisioning::urlencode_path as urlencode;

/// The permissions the application asks for.
///
/// Two, and the second is not optional in practice: GitHub requires
/// `metadata: read` alongside almost anything else, so asking for it
/// explicitly is honest rather than redundant.
///
/// **Not `administration`.** Workspec's equivalent asks for it because it
/// creates repositories; creating repositories is a stated non-goal here, and
/// an application that can create them can also rename and transfer them.
fn permissions() -> Value {
    json!({
        "contents": "write",
        "metadata": "read",
    })
}

/// Builds the application manifest.
///
/// Both callback URLs are derived from `callback_base` rather than passed in.
/// The host returns the browser to the *stored* setup URL after an install, so
/// the two have to agree, and deriving them together is what makes that true
/// by construction rather than by two call sites matching.
pub(super) fn build(callback_base: &str) -> Value {
    json!({
        // Globally unique across GitHub, so it carries the host this platform
        // answers on. Two SaaS Fabric deployments must be able to create their
        // own applications without one of them failing on a name clash.
        "name": name_for(callback_base),
        "url": callback_base,
        "redirect_url": format!("{callback_base}/api/integrations/git/created"),
        "setup_url": format!("{callback_base}/api/integrations/git/installed"),

        // The host returns the browser here again on *re-installation*, so an
        // operator changing which repositories are shared lands back in the
        // console rather than on a GitHub page with nowhere to go.
        "setup_on_update": true,

        // Nobody else may install this application: it exists to give one
        // platform access to one organisation's client configuration.
        "public": false,

        "default_permissions": permissions(),

        // None. The control plane is published on the operator plane and on no
        // public one, so a hook would be a URL GitHub can never deliver to.
        "default_events": [],
    })
}

/// The application name, which must be unique across the whole host.
fn name_for(callback_base: &str) -> String {
    let host = callback_base
        .rsplit("://")
        .next()
        .unwrap_or(callback_base)
        .split('/')
        .next()
        .unwrap_or(callback_base);

    format!("SaaS Fabric — {host}")
}

/// Where the operator's browser posts the manifest.
///
/// Organisation-scoped: the application belongs to the organisation whose
/// client configuration it will read, not to the person who happened to set it
/// up. An application owned by a personal account leaves with that person.
pub(super) fn creation_url(web_base: &str, organisation: &str, state: &str) -> String {
    format!(
        "{web_base}/organizations/{}/settings/apps/new?state={}",
        urlencode(organisation),
        urlencode(state)
    )
}

/// Where the operator's browser installs the application.
pub(super) fn install_url(web_base: &str, app_slug: &str, state: &str) -> String {
    format!(
        "{web_base}/apps/{}/installations/new?state={}",
        urlencode(app_slug),
        urlencode(state)
    )
}
