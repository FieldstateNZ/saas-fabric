//! The control-plane API's handlers.
//!
//! Every one of them is short on purpose. A handler here reads a path
//! parameter, calls one method on [`ClientService`](crate::ClientService), and
//! renders the result — the rules live in the service, where there is exactly
//! one copy of each.
//!
//! Every handler takes an [`Operator`](crate::Operator), including the ones
//! that only read. That is not decoration: the extractor is what performs
//! authentication, so a handler without the parameter would be a handler
//! anybody could call.

mod get_client;
mod get_identity;
mod list_clients;
mod put_identity;

pub(crate) use get_client::get_client;
pub(crate) use get_identity::get_identity;
pub(crate) use list_clients::list_clients;
pub(crate) use put_identity::put_identity;
