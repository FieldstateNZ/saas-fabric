//! Starts postgres on the run's network, waits for it, and seeds the shared
//! table.

use std::time::Duration;

use crate::support::docker;
use crate::support::images;
use crate::support::names::RunId;

/// The user, password, and database every run's postgres container answers
/// to. Fixed rather than randomised: nothing here needs the value
/// unpredictable, and a fixed value is one less thing to thread through
/// [`connection_uri`].
pub const USER: &str = "fabric";
/// See [`USER`].
pub const PASSWORD: &str = "fabric";
/// See [`USER`].
pub const DB: &str = "fabric";

/// How long to wait for `pg_isready` before giving up.
const READY_DEADLINE: Duration = Duration::from_secs(30);

/// The shared table: one physical row per tenant under the same logical
/// `id` -- the whole fixture the isolation proof this issue exists for
/// turns on. Seeded as SQL literals, never from a Rust constant: a mutation
/// to a tenant's published binding must not be able to move this corpus
/// with it (`docs/verification.md` row 1a; `m2-ndc/plan.md` §3.4).
const SEED_SQL: &str = "\
CREATE TABLE articles (id text NOT NULL, tenant_key text NOT NULL, title text NOT NULL, body text, PRIMARY KEY (id, tenant_key));
INSERT INTO articles (id, tenant_key, title, body) VALUES
  ('1', 'tenant-acme-482', 'Acme Handbook', NULL),
  ('1', 'tenant-globex-915', 'Globex Playbook', NULL);
";

/// Starts postgres, waits for it to accept connections, and seeds the
/// `articles` table.
pub fn start(run_id: &RunId, network: &str) -> docker::Container {
    let container = docker::run(&docker::RunSpec {
        name: run_id.container_name("postgres"),
        image: images::POSTGRES.to_owned(),
        network: network.to_owned(),
        env: vec![
            ("POSTGRES_USER".to_owned(), USER.to_owned()),
            ("POSTGRES_PASSWORD".to_owned(), PASSWORD.to_owned()),
            ("POSTGRES_DB".to_owned(), DB.to_owned()),
        ],
        publish: None,
        mount_ro: None,
        command: Vec::new(),
    })
    .unwrap_or_else(|error| panic!("could not start postgres: {error}"));

    wait_ready(&container);
    seed(&container);
    container
}

/// Polls to a deadline, past the official image's own startup quirk: it
/// runs a temporary server to execute init scripts, shuts it down, then
/// starts the real one -- so `pg_isready` alone can catch the temporary
/// server's brief ready window and report success just before the socket
/// disappears out from under a caller (observed directly: seeding raced
/// this window and got `No such file or directory`). The log line
/// `database system is ready to accept connections` appears once for each
/// server, so waiting for it twice, then re-confirming with `pg_isready`,
/// is what actually means "the real server is the one now listening".
fn wait_ready(container: &docker::Container) {
    let ready = docker::poll_until(READY_DEADLINE, || {
        let restarted = docker::logs(container).is_ok_and(|log| {
            log.matches("database system is ready to accept connections")
                .count()
                >= 2
        });

        restarted
            && docker::exec(container, &["pg_isready", "-U", USER, "-d", DB])
                .is_ok_and(|output| output.status.success())
    });
    assert!(ready, "postgres did not become ready within {READY_DEADLINE:?}");
}

/// Runs [`SEED_SQL`] through `psql`, over stdin rather than `-c`, so the
/// multi-statement script runs as one call.
fn seed(container: &docker::Container) {
    let output = docker::exec_with_stdin(
        container,
        &["psql", "-U", USER, "-d", DB, "-v", "ON_ERROR_STOP=1"],
        SEED_SQL.as_bytes(),
    )
    .unwrap_or_else(|error| panic!("could not run psql to seed articles: {error}"));

    docker::ensure_success("docker exec ... psql (seed articles)", &output)
        .unwrap_or_else(|error| panic!("seeding the shared table failed: {error}"));
}

/// The connection URI the connector container should use to reach `container`
/// -- by its Docker network name, since both containers share
/// [`RunId::network_name`].
#[must_use]
pub fn connection_uri(container: &docker::Container) -> String {
    format!("postgresql://{USER}:{PASSWORD}@{}:5432/{DB}", container.name())
}

/// Runs `sql` and returns its first column, first row, as text.
///
/// For assertions that must read the database directly rather than through
/// the connector under test -- "the row really exists" cannot be proven by
/// the same connector whose predicate is what is being tested.
#[must_use]
pub fn query_scalar(container: &docker::Container, sql: &str) -> String {
    let output = docker::exec(container, &["psql", "-U", USER, "-d", DB, "-t", "-A", "-c", sql])
        .unwrap_or_else(|error| panic!("could not run psql: {error}"));

    docker::ensure_success("docker exec ... psql (query_scalar)", &output)
        .unwrap_or_else(|error| panic!("query_scalar({sql:?}) failed: {error}"));

    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}
