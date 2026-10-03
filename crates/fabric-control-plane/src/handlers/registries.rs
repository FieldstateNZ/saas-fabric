//! `/api/integrations/registries` — the image registries an operator
//! registers (ADR 0026 section 5).
//!
//! # What no handler here accepts, or returns
//!
//! A registry is named by its host, which selects a record this platform
//! holds; no handler accepts a URL to read, and only `distribution`'s
//! registration takes an endpoint, which is checked and fixed there. A token
//! arrives in a request body and goes nowhere but the secret partition: no
//! response, log line or audit record carries it, and there is no route that
//! reveals it — unlike a client's secret, a registry credential has no
//! reveal.
//!
//! # Why the kind is chosen by the route family
//!
//! There is still no `/api/integrations/{kind}`: registries are one product
//! concept with their own fixed path, and which registry is a host in it, not
//! a name that could select another integration.

mod admitted;
mod credential;
mod list;
mod path;
mod read_by;
mod reads;
mod register;
mod remove;
mod repositories;
mod versions;
mod view;

pub(crate) use credential::{remove_registry_credential, set_registry_credential};
pub(crate) use list::{list_registries, registry_reads};
pub(crate) use register::register_registry;
pub(crate) use remove::remove_registry;
pub(crate) use repositories::{add_registry_repository, remove_registry_repository};
pub(crate) use versions::registry_versions;
