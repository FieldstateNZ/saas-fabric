//! Keeping the reader-generation listener a separate, explicit socket.

use std::net::SocketAddr;

use crate::config::AppConfig;

/// Refuses a reader-generation address that is not an explicit `ip:port`,
/// that asks for an ephemeral port, or that shares the main listener's port.
///
/// The listener is unauthenticated, so it must never be the main one or
/// overlap it: a shared port would put its route beside the Data API and the
/// probes, or fail at bind after the runtime has primed.
pub(super) fn validate(config: &AppConfig) -> Result<(), String> {
    let Some(address) = &config.reader_generation_listen else {
        return Ok(());
    };
    let reader: SocketAddr = address
        .parse()
        .map_err(|_| "reader_generation_listen must be an explicit ip:port".to_owned())?;
    if reader.port() == 0 {
        return Err("reader_generation_listen must name a port, not ask for an ephemeral one".to_owned());
    }
    let shares_main_port = config
        .listen
        .parse::<SocketAddr>()
        .is_ok_and(|main| main.port() == reader.port());
    if shares_main_port {
        return Err(
            "reader_generation_listen must use a port of its own: the main listener serves the Data API \
             and the probes, and this listener is unauthenticated"
                .to_owned(),
        );
    }
    Ok(())
}
