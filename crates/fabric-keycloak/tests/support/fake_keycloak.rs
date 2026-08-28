//! A minimal HTTP server that behaves enough like Keycloak.

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

    /// Whether the request carried a bearer token.
    pub authorised: bool,

    /// The bearer it carried, if any.
    ///
    /// Recorded rather than only counted so a test can tell a *stale* token
    /// from a fresh one — which is the whole subject of the refusal-retry
    /// behaviour.
    pub bearer: Option<String>,
}

/// A stand-in Keycloak.
pub struct FakeKeycloak {
    /// Where it is listening, as a base URL.
    pub base_url: String,

    /// Every request it received, in order.
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

/// What the fake answers for a given method and path prefix.
type Responder = Arc<dyn Fn(&RecordedRequest) -> (u16, String) + Send + Sync>;

impl FakeKeycloak {
    /// Starts a fake Keycloak that answers through `responder`.
    ///
    /// The token endpoint is answered by the fake itself, so no test has to
    /// restate the client-credentials exchange — but it is still a real
    /// request over the socket, so an adapter that forgot to make it would
    /// fail on the first admin call rather than silently succeeding.
    pub async fn start(responder: Responder) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);

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

        Self {
            base_url: format!("http://{address}"),
            requests,
        }
    }

    /// Every request received so far.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// The requests that were not the token exchange.
    pub fn admin_requests(&self) -> Vec<RecordedRequest> {
        self.requests()
            .into_iter()
            .filter(|request| request.path.starts_with("/admin/"))
            .collect()
    }

    /// How many times a method and path prefix were requested.
    pub fn count(&self, method: &str, prefix: &str) -> usize {
        self.requests()
            .iter()
            .filter(|request| request.method == method && request.path.starts_with(prefix))
            .count()
    }
}

/// Serves every request on one keep-alive connection.
async fn serve(mut stream: TcpStream, responder: &Responder, recorded: &Arc<Mutex<Vec<RecordedRequest>>>) {
    let mut buffer = Vec::new();

    loop {
        let Some(request) = read_request(&mut stream, &mut buffer).await else {
            return;
        };
        recorded.lock().unwrap().push(request.clone());

        let (status, body) = if request.path.contains("/protocol/openid-connect/token") {
            // A different token per mint, so a test can assert that a *fresh*
            // one was presented rather than merely that a request was retried.
            // The in-flight request is already recorded, so the first mint is
            // `test-token-1`.
            let minted = recorded
                .lock()
                .unwrap()
                .iter()
                .filter(|seen| seen.path.contains("/protocol/openid-connect/token"))
                .count();

            (
                200,
                format!(r#"{{"access_token":"test-token-{minted}","expires_in":300}}"#),
            )
        } else {
            responder(&request)
        };

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

    let bearer = headers
        .get("authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(ToOwned::to_owned);

    Some(RecordedRequest {
        method,
        path,
        body,
        authorised: bearer.is_some(),
        bearer,
    })
}

/// Finds a byte sequence in a buffer.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}
