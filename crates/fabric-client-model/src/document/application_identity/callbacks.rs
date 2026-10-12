//! Which redirect block one assignment projects for one client: the kind of
//! the client's declared hosts decides it (D01-15).

use crate::identity::is_private_network;
use crate::{ClientId, DesiredStateError, Host, RedirectStrategy, RedirectStrategyKind, RedirectUri};

use super::identifier;

/// What an operator is told when a client's hosts disagree about their kind.
const MIXED_HOSTS: &str = "An assigned application follows the kind of the client's hosts, and these \
     mix .internal and public hostnames: one projected client holds one redirect strategy, so \
     declare only .internal hosts or only public ones";

/// What an operator is told when there is nothing to build a callback on.
const NO_CALLBACK: &str = "An assigned application requires a client hostname or application domain";

/// The redirect block `application` is projected with for this client.
///
/// A client whose hosts are all `.internal` gets `privateNetwork` callbacks
/// on those hosts and nothing else. The release's hostname template is
/// always a public name — publication refuses a `.internal` one — so
/// projecting it would give a private-network client a public callback,
/// which D01-15 rules out. The callbacks are plain HTTP because that is how
/// the private network these hosts sit on is reached: no publicly-trusted
/// certificate can exist for `.internal`, and the deployment running on it
/// today has one gateway listener, on port 80. An `https` callback there could
/// only be made to work by changing the network, which D01-15 also rules out.
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

    let (kind, scheme) = if private > 0 {
        (RedirectStrategyKind::PrivateNetwork, "http")
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
