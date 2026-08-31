//! `/api/clients/{clientId}/secrets` — one client's secrets.
//!
//! # What no handler here accepts
//!
//! A namespace, a mount, a store address, a token, or a policy. An operator
//! names a client and a path within it; everything else is trusted platform
//! state resolved from the client's desired state. There is no field to omit
//! checking, because there is no field.
//!
//! # Reveal is a `POST`
//!
//! Not because it changes anything — it does not — but because it is an act.
//! A `GET` invites a browser, a proxy or a history entry to repeat it, and
//! this is the one operation whose repetition is worth noticing. It is also
//! the only response that must never be stored, so it sets `Cache-Control:
//! no-store` explicitly rather than trusting a default.

mod delete;
mod list;
mod reveal;
mod write;

pub(crate) use delete::delete_secret;
pub(crate) use list::{list_secrets, secret_metadata};
pub(crate) use reveal::reveal_secret;
pub(crate) use write::write_secret;
