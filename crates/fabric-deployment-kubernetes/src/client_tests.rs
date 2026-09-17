#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Token(PathBuf);
impl Drop for Token {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn client(responses: Vec<(u16, String)>) -> (Client, Token, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                request.extend_from_slice(&byte);
            }
            requests.push(String::from_utf8(request).unwrap());
            write!(
                socket,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        requests
    });
    let token = Token(std::env::temp_dir().join(format!(
        "fabric-observation-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::write(&token.0, "first-test-token").unwrap();
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
#[tokio::test]
async fn credentials_rotate_and_only_get_requests_are_sent() {
    let (client, token, task) = client(vec![(200, "{}".into()), (200, "{}".into())]);
    let _: serde_json::Value = client.get("/first", None).await.unwrap();
    std::fs::write(&token.0, "second-test-token\n").unwrap();
    let _: serde_json::Value = client.get("/second", None).await.unwrap();
    let requests = task.join().unwrap();
    assert!(requests.first().unwrap().starts_with("GET /first "));
    assert!(requests.first().unwrap().contains("Bearer first-test-token"));
    assert!(requests.last().unwrap().contains("Bearer second-test-token"));
    assert!(!requests.last().unwrap().contains("first-test-token"));
}
#[tokio::test]
async fn errors_do_not_expose_cluster_response_or_credentials() {
    let (client, _token, task) = client(vec![(403, "private cluster detail".into())]);
    let error = client
        .get::<serde_json::Value>("/deployment", None)
        .await
        .unwrap_err();
    assert_eq!(error, "Deployment observation is not permitted.");
    task.join().unwrap();
}
#[tokio::test]
async fn refuses_partial_lists_and_encodes_selectors() {
    let (client, _token, task) = client(vec![(
        200,
        r#"{"metadata":{"continue":"next"},"items":[]}"#.into(),
    )]);
    let error = client
        .list::<serde_json::Value>("/pods", "app=fabric,role=api")
        .await
        .unwrap_err();
    assert_eq!(error, "Deployment evidence requires more than one page.");
    let requests = task.join().unwrap();
    assert!(requests
        .first()
        .unwrap()
        .contains("labelSelector=app%3Dfabric%2Crole%3Dapi&limit=500"));
}
