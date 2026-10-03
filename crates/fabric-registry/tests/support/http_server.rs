//! A minimal HTTP/1.1 server for tests.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// One request the fake received.
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    /// The HTTP method.
    pub method: String,

    /// The path, including any query string.
    pub path: String,

    /// The request body, as sent.
    pub body: String,

    /// The `Authorization` header, if one was sent.
    pub authorization: Option<String>,

    /// The `Accept` header, if one was sent.
    pub accept: Option<String>,
}

/// What a fake answers with.
///
/// Headers, not just a body: this adapter checks a manifest's digest against
/// `Docker-Content-Digest`, follows pagination through `Link`, tells an index
/// from anything else by `Content-Type`, and follows a blob's redirect
/// through `Location`, so a server that could only return a body could not
/// exercise any of them.
pub struct Reply {
    /// The status code.
    pub status: u16,

    /// Response headers, beyond the ones every reply carries.
    pub headers: Vec<(String, String)>,

    /// The body.
    pub body: String,

    /// Whether the body is sent chunked, with no `Content-Length`: the only
    /// way a reader's bound is exercised as the body arrives rather than
    /// against a declared length.
    pub unlengthed: bool,
}

impl Reply {
    /// A JSON reply with no extra headers.
    pub fn json(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
            unlengthed: false,
        }
    }

    /// The same, with one header added.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }
}

/// What a fake answers with.
pub type Responder = Arc<dyn Fn(&RecordedRequest) -> Reply + Send + Sync>;

/// Starts a server on an ephemeral port and returns its base URL.
pub async fn start(responder: Responder, recorded: Arc<Mutex<Vec<RecordedRequest>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let responder = Arc::clone(&responder);
            let recorded = Arc::clone(&recorded);

            tokio::spawn(async move { serve(stream, &responder, &recorded).await });
        }
    });

    format!("http://{address}")
}

/// Serves every request on one keep-alive connection.
async fn serve(mut stream: TcpStream, responder: &Responder, recorded: &Arc<Mutex<Vec<RecordedRequest>>>) {
    let mut buffer = Vec::new();

    loop {
        let Some(request) = read_request(&mut stream, &mut buffer).await else {
            return;
        };
        recorded.lock().unwrap().push(request.clone());

        let reply = responder(&request);
        let mut extra = String::new();
        // JSON unless the reply names its own type, as a registry names a
        // manifest's.
        if !reply
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        {
            extra.push_str("Content-Type: application/json\r\n");
        }
        for (name, value) in &reply.headers {
            extra.push_str(name);
            extra.push_str(": ");
            extra.push_str(value);
            extra.push_str("\r\n");
        }
        // A `HEAD` is answered with the headers a `GET` would have, and no
        // body: sending one would be read as the start of the next response.
        let body = if request.method == "HEAD" {
            ""
        } else {
            reply.body.as_str()
        };
        let response = if reply.unlengthed {
            let chunks = if body.is_empty() {
                "0\r\n\r\n".to_owned()
            } else {
                format!("{:x}\r\n{body}\r\n0\r\n\r\n", body.len())
            };
            let chunks = if request.method == "HEAD" {
                ""
            } else {
                chunks.as_str()
            };
            format!(
                "HTTP/1.1 {} X\r\n{extra}Transfer-Encoding: chunked\r\n\r\n{chunks}",
                reply.status,
            )
        } else {
            format!(
                "HTTP/1.1 {} X\r\n{extra}Content-Length: {}\r\n\r\n{body}",
                reply.status,
                reply.body.len(),
            )
        };

        if stream.write_all(response.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// Reads one request off the connection, leaving any surplus in `buffer`.
async fn read_request(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> Option<RecordedRequest> {
    let head_end = loop {
        if let Some(position) = find(buffer, b"\r\n\r\n") {
            break position + 4;
        }

        let mut chunk = [0_u8; 4096];
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(chunk.get(..read)?);
    };

    let head = String::from_utf8_lossy(buffer.get(..head_end)?).to_string();
    let mut lines = head.lines();
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_owned();
    let path = request_line.next()?.to_owned();

    let headers: BTreeMap<String, String> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_owned()))
        .collect();

    let length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    while buffer.len() < head_end + length {
        let mut chunk = [0_u8; 4096];
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(chunk.get(..read)?);
    }

    let body = String::from_utf8_lossy(buffer.get(head_end..head_end + length)?).to_string();
    buffer.drain(..head_end + length);

    Some(RecordedRequest {
        method,
        path,
        body,
        authorization: headers.get("authorization").cloned(),
        accept: headers.get("accept").cloned(),
    })
}

/// Finds a byte sequence in a buffer.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
