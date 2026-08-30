//! An authorization service that answers over a real socket.
//!
//! # Why a socket rather than a mocked client
//!
//! What is worth testing is what actually goes out: the store in the path, the
//! model and the tuple in the body, and the exact strings a `SubjectId`, a
//! `RelationName` and an `ObjectRef` render to. A mocked client would let a
//! transformation creep in anywhere between the operation and the wire and
//! every test would still pass.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

/// One request the fake received.
#[derive(Debug, Clone)]
pub struct Recorded {
    /// The request path, which carries the store id.
    pub path: String,

    /// The body, parsed.
    pub body: serde_json::Value,
}

impl Recorded {
    /// A string field from the tuple key.
    pub fn tuple(&self, field: &str) -> String {
        self.body["tuple_key"][field]
            .as_str()
            .unwrap_or_else(|| panic!("tuple_key.{field} should be a string"))
            .to_owned()
    }

    /// The authorization model the request named, if any.
    pub fn model(&self) -> Option<String> {
        self.body["authorization_model_id"]
            .as_str()
            .map(std::borrow::ToOwned::to_owned)
    }
}

/// A stand-in authorization service on loopback.
pub struct FakeOpenFga {
    /// The port it is listening on.
    pub port: u16,

    /// Every request it received, in order.
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl FakeOpenFga {
    /// Starts a fake answering every request with `status` and `body`.
    pub async fn answering(status: u16, body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));

        let recorded = Arc::clone(&requests);
        let body = body.to_owned();

        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };

                let mut buffer = vec![0_u8; 8192];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();

                if let Some(entry) = parse(&request) {
                    recorded.lock().unwrap().push(entry);
                }

                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );

                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });

        Self { port, requests }
    }

    /// The single request received, panicking if there was not exactly one.
    pub fn only_request(&self) -> Recorded {
        let held = self.requests.lock().unwrap();

        assert_eq!(held.len(), 1, "expected exactly one request");

        held[0].clone()
    }

    /// How many requests it received.
    pub fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

/// Pulls the path and JSON body out of a raw request.
fn parse(request: &str) -> Option<Recorded> {
    let (head, body) = request.split_once("\r\n\r\n")?;
    let path = head.lines().next()?.split_whitespace().nth(1)?.to_owned();

    Some(Recorded {
        path,
        body: serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
    })
}
