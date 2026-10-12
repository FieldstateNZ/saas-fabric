//! The probe presents nothing, follows nothing, and reads only a bounded,
//! well-formed report in bounded time (G7). Real sockets on loopback.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::thread::JoinHandle;
use std::time::Duration;

use super::*;

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// Accepts one connection, answers it with `response` after `delay`, and
/// returns the request it read.
fn pod(response: String, delay: Duration) -> (u16, JoinHandle<String>) {
    let listener = TcpListener::bind((LOOPBACK, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).unwrap();
            request.extend_from_slice(&byte);
        }
        std::thread::sleep(delay);
        let _ = socket.write_all(response.as_bytes());
        String::from_utf8(request).unwrap()
    });
    (port, task)
}

fn answer(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
        body.len()
    )
}

fn report(contract: u32, generation: u64) -> String {
    format!(
        r#"{{"contract":{contract},"generation":{generation},"record_sha256":"{}","origin":"published"}}"#,
        "ab".repeat(32)
    )
}

async fn ask(response: String) -> (ProbeOutcome, String) {
    let (port, task) = pod(response, Duration::ZERO);
    let outcome = HttpProbe::new(port).unwrap().ask(LOOPBACK).await;
    (outcome, task.join().unwrap())
}

#[tokio::test]
async fn a_report_is_read_with_a_plain_get_that_carries_no_credential() {
    let (outcome, request) = ask(answer("200 OK", "", &report(1, 7))).await;
    let ProbeOutcome::Reported(report) = outcome else {
        panic!("expected a report, got {outcome:?}");
    };
    assert_eq!(report.generation, 7);
    assert_eq!(report.record_sha256.to_string(), "ab".repeat(32));
    assert!(
        request.starts_with("GET /reader-generation HTTP/1.1\r\n"),
        "{request}"
    );
    let lowered = request.to_ascii_lowercase();
    for header in ["authorization", "cookie", "proxy-authorization", "bearer"] {
        assert!(!lowered.contains(header), "{request}");
    }
}

#[tokio::test]
async fn nothing_served_is_its_own_answer() {
    assert_eq!(
        ask(answer("404 Not Found", "", "")).await.0,
        ProbeOutcome::NothingServed
    );
}

#[tokio::test]
async fn a_redirect_is_never_followed() {
    let elsewhere = TcpListener::bind((LOOPBACK, 0)).unwrap();
    elsewhere.set_nonblocking(true).unwrap();
    let location = format!(
        "Location: http://{}/reader-generation\r\n",
        elsewhere.local_addr().unwrap()
    );
    let (outcome, _) = ask(answer("302 Found", &location, "")).await;
    assert_eq!(outcome, ProbeOutcome::Failed(ProbeFailure::Redirected));
    assert!(elsewhere.accept().is_err(), "the redirect target was contacted");
}

#[tokio::test]
async fn another_status_is_reported_by_number() {
    let (outcome, _) = ask(answer("500 Internal Server Error", "", "")).await;
    assert_eq!(outcome, ProbeOutcome::Failed(ProbeFailure::Status(500)));
}

#[tokio::test]
async fn an_answer_past_the_size_bound_is_refused() {
    let padded = format!("{}{}", report(1, 7), " ".repeat(READER_GENERATION_MAX_BYTES));
    let (outcome, _) = ask(answer("200 OK", "", &padded)).await;
    assert_eq!(outcome, ProbeOutcome::Failed(ProbeFailure::TooLarge));
}

#[tokio::test]
async fn an_answer_that_is_not_a_report_this_side_reads_is_malformed() {
    let extra = report(1, 7).replace('}', r#","tenants":["acme"]}"#);
    for body in [
        "{}".to_owned(),
        "not json".to_owned(),
        extra,
        report(2, 7),
        report(1, 0),
    ] {
        let (outcome, _) = ask(answer("200 OK", "", &body)).await;
        assert_eq!(outcome, ProbeOutcome::Failed(ProbeFailure::Malformed), "{body}");
    }
}

#[tokio::test]
async fn a_pod_that_does_not_answer_in_time_is_timed_out() {
    let (port, task) = pod(
        answer("200 OK", "", &report(1, 7)),
        PROBE_TIMEOUT + Duration::from_secs(1),
    );
    let outcome = HttpProbe::new(port).unwrap().ask(LOOPBACK).await;
    assert_eq!(outcome, ProbeOutcome::Failed(ProbeFailure::TimedOut));
    task.join().unwrap();
}

#[tokio::test]
async fn a_pod_nothing_listens_on_is_unreachable() {
    let port = TcpListener::bind((LOOPBACK, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let outcome = HttpProbe::new(port).unwrap().ask(LOOPBACK).await;
    assert_eq!(outcome, ProbeOutcome::Failed(ProbeFailure::Unreachable));
}
