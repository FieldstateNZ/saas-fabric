//! A minimal HTTP/1.1 server for tests.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

    /// When the server finished reading it, which is as close to "when the
    /// client sent it" as this side can observe.
    ///
    /// Here so a test can assert that no request *started* after an operation's
    /// budget expired — the property that separates "refused to begin another
    /// call" from "dropped the call it had already made".
    pub at: Instant,
}

/// What a fake answers with.
pub type Responder = Arc<dyn Fn(&RecordedRequest) -> (u16, String) + Send + Sync>;

/// How long the server sits on a request before answering it.
///
/// A slow *answer*, not a dead connection: the request is read, the response is
/// built, and only the writing of it waits. That is the shape a budget has to
/// cope with — everything below HTTP is healthy, so nothing errors and nothing
/// retries, and the call simply takes longer than the caller hoped.
pub type Delay = Arc<dyn Fn(&RecordedRequest) -> Duration + Send + Sync>;

/// What the server saw, and what it finished.
#[derive(Clone)]
pub struct Traffic {
    /// Every request, in the order they were read.
    pub requests: Arc<Mutex<Vec<RecordedRequest>>>,

    /// `METHOD path` for each request whose response was written in full.
    ///
    /// Separate from `requests` because the difference between the two is the
    /// whole question: a request that was started and a request that ran to its
    /// outcome are the same entry in one list and different entries in both.
    pub completed: Arc<Mutex<Vec<String>>>,
}

impl Traffic {
    /// An empty log.
    pub fn new() -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            completed: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for Traffic {
    fn default() -> Self {
        Self::new()
    }
}

/// Starts a server on an ephemeral port and returns its base URL.
pub async fn start(responder: Responder, traffic: Traffic) -> String {
    start_delaying(responder, traffic, Arc::new(|_| Duration::ZERO)).await
}

/// The same, with a per-request delay before the answer is written.
pub async fn start_delaying(responder: Responder, traffic: Traffic, delay: Delay) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let responder = Arc::clone(&responder);
            let delay = Arc::clone(&delay);
            let traffic = traffic.clone();

            tokio::spawn(async move { serve(stream, &responder, &delay, &traffic).await });
        }
    });

    format!("http://{address}")
}

/// Serves every request on one keep-alive connection.
async fn serve(mut stream: TcpStream, responder: &Responder, delay: &Delay, traffic: &Traffic) {
    let mut buffer = Vec::new();

    loop {
        let Some(request) = read_request(&mut stream, &mut buffer).await else {
            return;
        };
        traffic.requests.lock().unwrap().push(request.clone());

        let (status, body) = responder(&request);
        let response = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );

        tokio::time::sleep(delay(&request)).await;

        if stream.write_all(response.as_bytes()).await.is_err() {
            return;
        }

        traffic
            .completed
            .lock()
            .unwrap()
            .push(format!("{} {}", request.method, request.path));
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
        at: Instant::now(),
    })
}

/// Finds a byte sequence in a buffer.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
