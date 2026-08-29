//! An OpenBao that answers over a real socket.
//!
//! # Why a socket rather than a mocked client
//!
//! Everything worth testing here is protocol: whether the adapter logs in
//! before it reads, presents the token in the header OpenBao expects, unwraps
//! a version 2 entry's double nesting, reads a `404` as absence, deletes
//! through `metadata` rather than `data`, and logs in again when a token is
//! refused. A mocked client would let every one of those be wrong and every
//! test still pass.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// One request the fake received.
#[derive(Debug, Clone)]
pub struct Recorded {
    /// The HTTP method.
    pub method: String,

    /// The path.
    pub path: String,

    /// The body, as sent.
    pub body: String,

    /// The store token presented, if any.
    pub token: Option<String>,
}

/// A stand-in OpenBao.
pub struct FakeOpenBao {
    /// Where it is listening.
    pub address: String,

    /// Every request it received, in order.
    requests: Arc<Mutex<Vec<Recorded>>>,

    /// How many logins it has served.
    logins: Arc<Mutex<usize>>,
}

/// What the fake answers for a data or metadata request.
type Responder = Arc<dyn Fn(&Recorded) -> (u16, String) + Send + Sync>;

impl FakeOpenBao {
    /// Starts a fake that answers data requests through `responder`.
    ///
    /// Logins are answered by the fake itself, so no test restates them — but
    /// they are still real requests over the socket, so an adapter that forgot
    /// to log in would fail rather than silently succeed.
    pub async fn start(responder: Responder) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let logins = Arc::new(Mutex::new(0));

        let recorded = Arc::clone(&requests);
        let counted = Arc::clone(&logins);

        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };

                let responder = Arc::clone(&responder);
                let recorded = Arc::clone(&recorded);
                let counted = Arc::clone(&counted);

                tokio::spawn(async move {
                    serve(stream, &responder, &recorded, &counted).await;
                });
            }
        });

        Self {
            address: format!("http://{address}"),
            requests,
            logins,
        }
    }

    /// Every request received.
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    /// How many times the adapter logged in.
    pub fn logins(&self) -> usize {
        *self.logins.lock().unwrap()
    }
}

/// Answers one connection.
async fn serve(
    mut stream: TcpStream,
    responder: &Responder,
    recorded: &Arc<Mutex<Vec<Recorded>>>,
    logins: &Arc<Mutex<usize>>,
) {
    let mut buffer = vec![0_u8; 65536];
    let read = stream.read(&mut buffer).await.unwrap_or(0);
    if read == 0 {
        return;
    }

    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
    let parsed = parse(&request);

    let (status, body) = if parsed.path.contains("/auth/") {
        *logins.lock().unwrap() += 1;
        (
            200,
            r#"{"auth":{"client_token":"a-store-token","lease_duration":3600}}"#.to_owned(),
        )
    } else {
        recorded.lock().unwrap().push(parsed.clone());
        responder(&parsed)
    };

    let response = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

/// Reads what this fake needs out of a raw request.
fn parse(request: &str) -> Recorded {
    let mut lines = request.lines();
    let start = lines.next().unwrap_or_default();
    let mut parts = start.split_whitespace();

    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();

    let mut headers = BTreeMap::new();
    for line in lines.by_ref() {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(": ") {
            headers.insert(name.to_ascii_lowercase(), value.to_owned());
        }
    }

    Recorded {
        method,
        path,
        body: request.split("\r\n\r\n").nth(1).unwrap_or_default().to_owned(),
        token: headers.get("x-vault-token").cloned(),
    }
}
