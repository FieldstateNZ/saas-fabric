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
}

/// What a fake answers with.
pub type Responder = Arc<dyn Fn(&RecordedRequest) -> (u16, String) + Send + Sync>;

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

        let (status, body) = responder(&request);
        let response = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );

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
    })
}

/// Finds a byte sequence in a buffer.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
