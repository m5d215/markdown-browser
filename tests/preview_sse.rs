use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use markdown_browser::cli::preview;

fn read_some(sock: &mut TcpStream, timeout: Duration) -> String {
    sock.set_read_timeout(Some(timeout)).unwrap();
    let mut buf = [0u8; 4096];
    let mut acc = Vec::new();
    loop {
        match sock.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                acc.extend_from_slice(&buf[..n]);
                if acc.len() > 8192 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&acc).into_owned()
}

fn parse_port(url: &str) -> u16 {
    url.rsplit(':').next().unwrap().parse().unwrap()
}

#[test]
fn sse_headers_arrive_before_first_event() {
    // Regression test for the buffering issue that hid the response status
    // until enough body bytes had accumulated, leaving EventSource stuck
    // on `connecting…` whenever the cursor wasn't sitting on a mermaid
    // block at startup.
    let handle = preview::start(None).expect("server start");
    let port = parse_port(handle.url());

    let mut sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
    sock.write_all(b"GET /events HTTP/1.1\r\nHost: localhost\r\nAccept: text/event-stream\r\n\r\n")
        .unwrap();

    let body = read_some(&mut sock, Duration::from_secs(2));
    assert!(
        body.contains("HTTP/1.1 200 OK"),
        "missing 200 status; got: {body:?}"
    );
    assert!(
        body.contains("text/event-stream"),
        "missing SSE content-type; got: {body:?}"
    );
}

#[test]
fn sse_flushes_event_to_client_immediately() {
    let mut handle = preview::start(None).expect("server start");
    let port = parse_port(handle.url());

    let mut sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
    sock.write_all(b"GET /events HTTP/1.1\r\nHost: localhost\r\nAccept: text/event-stream\r\n\r\n")
        .unwrap();

    // Give the server a moment to attach the subscriber.
    std::thread::sleep(Duration::from_millis(100));

    handle.set_source(Some("graph TD\nA-->B"));

    let body = read_some(&mut sock, Duration::from_secs(2));
    assert!(
        body.contains("HTTP/1.1 200 OK"),
        "missing 200 status; got: {body:?}"
    );
    assert!(
        body.contains("event: mermaid"),
        "missing mermaid event; got: {body:?}"
    );
    assert!(body.contains("A-->B"), "missing source; got: {body:?}");
}

#[test]
fn static_index_html_is_served() {
    let handle = preview::start(None).expect("server start");
    let port = parse_port(handle.url());
    let mut sock = TcpStream::connect(("127.0.0.1", port)).unwrap();
    sock.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let body = read_some(&mut sock, Duration::from_secs(2));
    assert!(body.contains("HTTP/1.1 200 OK"));
    assert!(body.contains("Mermaid Preview"));
}
