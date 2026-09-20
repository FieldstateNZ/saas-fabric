//! The control-plane API's handlers.
//!
//! Every one of them is short on purpose. A handler here reads a path
//! parameter, calls one method on [`ClientService`](crate::ClientService), and
//! renders the result — the rules live in the service, where there is exactly
//! one copy of each.
//!
//! Every handler that touches a client takes an
//! [`Operator`](crate::Operator), including the ones that only read. That is
//! not decoration: the extractor is what performs authentication, so a handler
//! without the parameter would be a handler anybody could call.
//!
//! The two in [`session`] are the deliberate exceptions, and they have to be:
//! they are how an operator obtains the token the extractor then demands.
//! Neither can read or change anything.

mod change_catalogue;
mod converge;
mod create_client;
mod get_catalogue;
mod get_client;
mod get_identity;
mod get_operator;
mod get_product;
mod get_runtime_catalogue;
mod integration;
mod list_activity;
mod list_clients;
mod placements;
mod platform;
mod put_identity;
mod put_product;
mod secrets;
mod secrets_path;
mod session;

pub(crate) use change_catalogue::change_catalogue;
pub(crate) use converge::converge;
pub(crate) use create_client::create_client;
pub(crate) use get_catalogue::get_catalogue;
pub(crate) use get_client::get_client;
pub(crate) use get_identity::get_identity;
pub(crate) use get_operator::get_operator;
pub(crate) use get_product::get_product;
pub(crate) use get_runtime_catalogue::get_runtime_catalogue;
pub(crate) use integration::{
    begin_connection, begin_install, choose_repository, created, disconnect, get_integration,
    get_platform_integration, installed, list_repositories, ClientConfigurationFlow, PlatformManagementFlow,
};
pub(crate) use list_activity::list_activity;
pub(crate) use list_clients::list_clients;
pub(crate) use placements::{list_placements, place_data_source};
pub(crate) use platform::{
    declare_data_source, get_platform, list_data_sources, pause_component, publish_runtime_state,
    remove_data_source, resume_component, roll_back_component, rollback_candidates,
};
pub(crate) use put_identity::put_identity;
pub(crate) use put_product::put_product;
pub(crate) use secrets::{delete_secret, list_secrets, reveal_secret, secret_metadata, write_secret};
pub(crate) use session::{redeem_session, session_config};
