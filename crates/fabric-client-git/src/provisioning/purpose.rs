//! What an application this platform creates is for.

/// The two things about a provisioned application that differ by purpose.
///
/// Everything else the manifest declares — the permissions it asks for, the
/// webhook it does not declare, being private — is the same for every
/// application this platform creates, and is stated once in
/// the manifest builder beside this. These two are not, and both are the
/// composition root's to supply:
///
/// - the **name**, because it is unique across the whole Git host. One
///   deployment creating two applications must not ask for the same name
///   twice, and the second request is the one that fails.
/// - the **callback segment**, because it has to agree with the routes that
///   serve those callbacks, and the routes are in the control plane. A crate
///   that guessed at them would be guessing about somebody else's URL space,
///   and the failure would show up as a browser landing on a 404 halfway
///   through a flow an operator had already approved on the host.
///
/// Neither is a policy this adapter is entitled to decide, which is why
/// neither has a default here.
#[derive(Clone, Debug)]
pub struct AppPurpose {
    /// The application's name, before the host this deployment answers on.
    pub name: String,

    /// The path segment its callbacks live under, as in
    /// `/api/integrations/<segment>/created`.
    pub callback_segment: String,
}
