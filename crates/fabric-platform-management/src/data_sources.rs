//! Declaring what an environment can place a tenant's data on.
//!
//! ADR 0023 part 1: a data source is environment desired state, declared by
//! an operator through the console and written by Fabric in one commit —
//! the same shape `components.yaml` already has, beside it in the platform
//! repository. Placement (part 2), the runtime catalogue (part 3) and the
//! publisher (part 4) are later slices and are not built here.

mod declaration;
mod held;
mod plan;
mod port;
mod read;
mod rule;
mod service;
mod validate;

pub use declaration::{DataSourceDeclaration, Discriminator};
pub use port::DataSourceState;
pub use read::DataSourcesRead;
pub use rule::{DataSourceRule, PoolField};
pub use service::{DataSources, Declared};
