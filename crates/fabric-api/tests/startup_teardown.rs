//! What a failed `startup::build` leaves running.
//!
//! `build_runtime` spawns two refreshers before the connectors are negotiated,
//! the catalogue is read, or the Data API is assembled. Returning `Err` from
//! any of those *dropped* the handles, and every one of those handles
//! documents that dropping it orphans its task rather than stopping it.
//!
//! In the binary the process exits moments later and the leak is invisible.
//! That is exactly why it needs a test: the property is real, and the only
//! thing hiding it is a coincidence of how `main` happens to use `build`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod examples_support;

use std::time::Duration;

use examples_support::{config, example};
use fabric_api::config::AppConfig;

/// The example configuration, with absolute state paths and connectors
/// pointed at a closed port so negotiation fails immediately.
fn config_that_cannot_negotiate() -> AppConfig {
    let mut config = config();

    config.tenants_path = example("tenants.json");
    config.data_sources_path = example("data-sources.json");
    config.catalog_path = example("catalog.json");

    for connector in &mut config.connectors {
        "http://127.0.0.1:1".clone_into(&mut connector.endpoint);
        connector.http_connect_timeout_seconds = 1;
        connector.http_timeout_seconds = 1;
    }

    config
}

/// How many tasks this runtime is currently keeping alive.
fn alive_tasks() -> usize {
    tokio::runtime::Handle::current().metrics().num_alive_tasks()
}

#[tokio::test]
async fn a_build_that_fails_after_priming_leaves_nothing_running() {
    let config = config_that_cannot_negotiate();

    let before = alive_tasks();

    let Err(error) = fabric_api::startup::build(&config).await else {
        panic!("negotiation against a closed port must fail the build");
    };
    assert!(
        error.contains("connector"),
        "the failure must be the connector step, or this test is measuring something else: {error}"
    );

    // Shutdown is cooperative — the refreshers are notified and then joined —
    // so give the runtime a moment to retire tasks that were told to stop.
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(
        alive_tasks(),
        before,
        "a failed build left background tasks polling a process that never started"
    );
}
