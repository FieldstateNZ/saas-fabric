//! A fake NDC connector on a real socket, for tests about delivery.
//!
//! Mocking the transport cannot express what these tests are about. The
//! failures the client must tell apart — a refused connect, a request that was
//! read and never answered, a body cut off after `200 OK` — are all the same
//! `reqwest::Error` type and differ only in how the TCP conversation ended. So
//! this speaks HTTP/1.1 by hand over a `TcpListener`.
//!
//! It counts the requests it read *in full* before misbehaving. That counter is
//! the point: it stands in for the mutation a real connector would already have
//! applied, and it makes "this write happened" a fact a test can assert rather
//! than a claim in a comment.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// How the fake connector ends the conversation once it holds the request.
#[derive(Debug, Clone, Copy)]
pub(super) enum Misbehaviour {
    /// Answer `200 OK` with a `Content-Length` it never satisfies, then close.
    /// The status line arrived, so the operation succeeded and only its result
    /// was lost.
    TruncateBodyAfterOk,
    /// Never answer at all, so the client's own total timeout ends the call.
    NeverAnswer,
    /// Close the connection without sending a status line.
    CloseWithoutAnswering,
}

/// A fake connector listening on an ephemeral port, serving one request.
pub(super) struct FakeConnector {
    endpoint: String,
    applied: Arc<AtomicUsize>,
}

impl FakeConnector {
    /// Starts one and returns as soon as it is listening.
    pub(super) async fn start(misbehaviour: Misbehaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let applied = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&applied);

        tokio::spawn(async move { serve_once(listener, misbehaviour, &counter).await });

        Self { endpoint, applied }
    }

    /// The base URL to point a client at.
    pub(super) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// How many requests were read in full — how many mutations a real
    /// connector would have applied by now.
    pub(super) fn applied(&self) -> usize {
        self.applied.load(Ordering::SeqCst)
    }
}

/// Accepts one connection, reads the whole request, then misbehaves.
async fn serve_once(listener: TcpListener, misbehaviour: Misbehaviour, applied: &AtomicUsize) {
    let Ok((mut socket, _)) = listener.accept().await else {
        return;
    };

    if !read_request(&mut socket).await {
        return;
    }

    // Counted before answering, which is the ordering a real connector has: the
    // write commits, and only then is it reported on.
    applied.fetch_add(1, Ordering::SeqCst);

    match misbehaviour {
        Misbehaviour::TruncateBodyAfterOk => {
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4096\r\n\r\n{\"operation_results\":")
                .await;
            let _ = socket.flush().await;
        }
        // `pending` rather than a sleep: nothing here should be racing the
        // client's timeout.
        Misbehaviour::NeverAnswer => std::future::pending::<()>().await,
        Misbehaviour::CloseWithoutAnswering => {}
    }
}

/// Reads the request headers and any declared body. `false` if the peer went
/// away first.
async fn read_request(socket: &mut TcpStream) -> bool {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let Ok(read) = socket.read(&mut chunk).await else {
            return false;
        };
        if read == 0 {
            return false;
        }
        buffer.extend_from_slice(chunk.get(..read).unwrap_or_default());

        if let Some(end) = headers_end(&buffer) {
            if buffer.len() >= end + content_length(&buffer) {
                return true;
            }
        }
    }
}

/// The offset just past the blank line ending the headers.
fn headers_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|w| w == b"\r\n\r\n").map(|at| at + 4)
}

/// The declared body length, or zero when none is declared.
fn content_length(buffer: &[u8]) -> usize {
    let headers = String::from_utf8_lossy(buffer).to_ascii_lowercase();

    headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}
