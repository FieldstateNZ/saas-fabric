//! A fake API server on a local socket, and a snapshot to publish to it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod snapshot;
pub(crate) use snapshot::snapshot;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::client::Client;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A recorded request: the request line, and the body if there was one.
pub(crate) struct Recorded {
    pub(crate) line: String,
    pub(crate) body: String,
    /// The bearer the request carried, if any.
    pub(crate) bearer: Option<String>,
}

pub(crate) struct Token(pub(crate) PathBuf);
impl Drop for Token {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Serves the scripted responses in order, one connection each, and hands
/// back every request it saw.
pub(crate) fn fake(responses: Vec<(u16, String)>) -> (Client, Token, std::thread::JoinHandle<Vec<Recorded>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut head = Vec::new();
            let mut byte = [0];
            while !head.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                head.extend_from_slice(&byte);
            }
            let head = String::from_utf8(head).unwrap();
            let length = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut body_bytes = vec![0; length];
            socket.read_exact(&mut body_bytes).unwrap();
            let bearer = head.lines().find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("authorization: bearer ")
                    .map(|_| {
                        l.split_once(' ')
                            .unwrap()
                            .1
                            .split_once(' ')
                            .unwrap()
                            .1
                            .trim()
                            .to_owned()
                    })
            });
            seen.push(Recorded {
                line: head.lines().next().unwrap().to_owned(),
                body: String::from_utf8(body_bytes).unwrap(),
                bearer,
            });
            write!(
                socket,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        seen
    });
    let token = Token(std::env::temp_dir().join(format!(
        "fabric-publication-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::write(&token.0, "test-token").unwrap();
    (
        Client {
            http: reqwest::Client::new(),
            base,
            token_file: token.0.clone(),
        },
        token,
        task,
    )
}

/// A held object's JSON, as the API server would answer a `GET`.
pub(crate) fn object(name: &str, version: &str, data: &[(&str, &str)]) -> String {
    let data: BTreeMap<&str, &str> = data.iter().copied().collect();
    serde_json::json!({"metadata": {"name": name, "resourceVersion": version}, "data": data}).to_string()
}
