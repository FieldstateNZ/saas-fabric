//! Which redirect block one assignment projects for one client: the kind of
//! the client's declared hosts decides the strategy, and the client's
//! declared scheme decides how its `.internal` callbacks are reached (D01-15).

use crate::identity::is_private_network;
use crate::{
    ClientId, DesiredStateError, Host, PrivateNetworkScheme, RedirectStrategy, RedirectStrategyKind,
    RedirectUri,
};

use super::identifier;

/// The field a scheme refusal names.
const SCHEME_FIELD: &str = "privateNetworkScheme";

/// What an operator is told when a client's hosts disagree about their kind.
const MIXED_HOSTS: &str = "An assigned application follows the kind of the client's hosts, and these \
     mix .internal and public hostnames: one projected client holds one redirect strategy, so \
     declare only .internal hosts or only public ones";

/// What an operator is told when there is nothing to build a callback on.
const NO_CALLBACK: &str = "An assigned application requires a client hostname or application domain";

/// What an operator is told when a client on `.internal` hosts has not said
/// how they are reached.
const UNDECLARED: &str = "A client on .internal hosts declares privateNetworkScheme, http or https, \
     before it can be assigned an application: an .internal name does not say whether its network \
     terminates TLS, and the identity provider matches a callback's scheme exactly";

/// What an operator is told when a scheme is declared for public hosts.
const MISPLACED: &str = "privateNetworkScheme applies only to a client whose hosts are all .internal; \
     a public host is always reached over https";

/// Refuses a declared scheme on a client that is not on `.internal` hosts
/// alone. Run on every product write, whether or not anything is assigned,
/// so the field never means anything but what its name says.
pub(in crate::document) fn check_scheme(
    hosts: &[Host],
    scheme: Option<PrivateNetworkScheme>,
) -> Result<(), DesiredStateError> {
    let internal = !hosts.is_empty() && hosts.iter().all(|host| is_private_network(host.as_str()));

    if scheme.is_some() && !internal {
        return Err(DesiredStateError::InvalidField {
            field: SCHEME_FIELD,
            detail: MISPLACED.into(),
        });
    }

    Ok(())
}

/// The redirect block `application` is projected with for this client.
///
/// A client whose hosts are all `.internal` gets `privateNetwork` callbacks
/// on those hosts, over the scheme it declares, and nothing else. The
/// release's hostname template is always a public name — publication refuses
/// a `.internal` one — so projecting it would give a private-network client a
/// hidden public entry, which D01-15 rules out. An undeclared scheme is
/// refused rather than guessed: see [`PrivateNetworkScheme`].
///
/// Every other client — no hosts, or only public ones — gets `claimedHttps`,
/// built exactly as before D01-15, so a stored projection on public hosts
/// stays its own projection.
///
/// A client mixing the two kinds is refused rather than resolved, for the
/// reason a `v1` document mixing callback kinds is: picking either strategy
/// would silently drop callbacks the operator declared or grant one they did
/// not.
pub(super) fn redirect(
    client: &ClientId,
    hosts: &[Host],
    scheme: Option<PrivateNetworkScheme>,
    application: &str,
    domain: &str,
) -> Result<RedirectStrategy, DesiredStateError> {
    let private = hosts
        .iter()
        .filter(|host| is_private_network(host.as_str()))
        .count();

    if private > 0 && private < hosts.len() {
        return Err(DesiredStateError::InvalidField {
            field: "hosts",
            detail: MIXED_HOSTS.into(),
        });
    }
    check_scheme(hosts, scheme)?;

    let (kind, scheme) = if private > 0 {
        let scheme = scheme.ok_or_else(|| DesiredStateError::InvalidField {
            field: SCHEME_FIELD,
            detail: UNDECLARED.into(),
        })?;
        (RedirectStrategyKind::PrivateNetwork, scheme.as_str())
    } else {
        (RedirectStrategyKind::ClaimedHttps, "https")
    };

    let mut uris = Vec::new();
    if kind == RedirectStrategyKind::ClaimedHttps && !domain.is_empty() {
        let host = domain.replace("{client}", client.as_str());
        uris.push(callback(format!("https://{host}/callback"))?);
    }
    for host in hosts {
        uris.push(callback(format!("{scheme}://{host}/{application}/callback"))?);
    }
    if uris.is_empty() {
        return Err(DesiredStateError::InvalidField {
            field: "hosts",
            detail: NO_CALLBACK.into(),
        });
    }

    RedirectStrategy::try_new(kind, uris)
}

fn callback(value: String) -> Result<RedirectUri, DesiredStateError> {
    RedirectUri::try_new(value).map_err(|error| identifier(&error))
}
