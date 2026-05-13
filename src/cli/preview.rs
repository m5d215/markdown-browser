//! Embedded HTTP server that streams the current mermaid block to a browser
//! tab over Server-Sent Events. See issue #35.
//!
//! Lifecycle:
//! - `start()` binds `127.0.0.1:port` (or an OS-assigned port when None),
//!   spawns an accept thread, and returns a [`PreviewHandle`] carrying the
//!   URL plus a sender for source updates.
//! - The TUI calls [`PreviewHandle::set_source`] each frame; identical
//!   updates are dropped at the handle level so callers don't need to
//!   memoise.
//! - The HTTP layer is hand-rolled to keep tight control over write
//!   buffering — every SSE event is flushed immediately so EventSource
//!   reports `open` as soon as headers go out and so cursor moves reach
//!   the browser without sitting in a buffer.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

const INDEX_HTML: &str = include_str!("../../assets/preview.html");
const MERMAID_JS: &[u8] = include_bytes!("../../assets/mermaid.min.js");

#[derive(Default)]
struct SharedState {
    last_event: Option<String>,
    subscribers: Vec<Sender<String>>,
}

pub struct PreviewHandle {
    url: String,
    state: Arc<Mutex<SharedState>>,
    last_pushed: Option<String>,
}

impl PreviewHandle {
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Push the given mermaid source to all connected browsers. Pass `None`
    /// to skip (cursor is not on a mermaid block); identical sources are
    /// deduplicated and become no-ops.
    pub fn set_source(&mut self, source: Option<&str>) {
        let Some(source) = source else { return };
        if self.last_pushed.as_deref() == Some(source) {
            return;
        }
        let event = format_event("mermaid", source);
        self.last_pushed = Some(source.to_string());
        let mut guard = self.state.lock().unwrap();
        guard
            .subscribers
            .retain(|tx| tx.send(event.clone()).is_ok());
        guard.last_event = Some(event);
    }
}

pub fn start(port: Option<u16>) -> io::Result<PreviewHandle> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port.unwrap_or(0)))?;
    let bound = listener.local_addr()?;
    let url = format!("http://{}:{}", bound.ip(), bound.port());
    let state = Arc::new(Mutex::new(SharedState::default()));
    let server_state = Arc::clone(&state);
    thread::Builder::new()
        .name("preview-server".into())
        .spawn(move || accept_loop(listener, server_state))?;
    Ok(PreviewHandle {
        url,
        state,
        last_pushed: None,
    })
}

fn accept_loop(listener: TcpListener, state: Arc<Mutex<SharedState>>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = Arc::clone(&state);
        // Each connection is handled in its own thread so SSE streams can
        // park on `rx.recv()` without blocking new requests.
        thread::spawn(move || {
            if let Err(_e) = handle_connection(stream, state) {
                // Client disconnects mid-response are routine; suppress.
            }
        });
    }
}

fn handle_connection(mut stream: TcpStream, state: Arc<Mutex<SharedState>>) -> io::Result<()> {
    // Disable Nagle so SSE events reach the browser without the 40 ms
    // delayed-ACK / Nagle interaction stretching them.
    let _ = stream.set_nodelay(true);

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    // Drain the rest of the request headers; we don't act on them.
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }

    if method != "GET" {
        return write_simple(
            &mut stream,
            405,
            "Method Not Allowed",
            b"method not allowed",
        );
    }

    match path {
        "/" | "/index.html" => write_static(
            &mut stream,
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes(),
            false,
        ),
        "/mermaid.min.js" => write_static(
            &mut stream,
            "application/javascript; charset=utf-8",
            MERMAID_JS,
            true,
        ),
        "/events" => handle_sse(stream, state),
        _ => write_simple(&mut stream, 404, "Not Found", b"not found"),
    }
}

fn write_simple(stream: &mut TcpStream, code: u16, reason: &str, body: &[u8]) -> io::Result<()> {
    let headers = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}

fn write_static(
    stream: &mut TcpStream,
    content_type: &str,
    body: &[u8],
    cacheable: bool,
) -> io::Result<()> {
    let cache = if cacheable {
        "Cache-Control: public, max-age=31536000, immutable\r\n"
    } else {
        "Cache-Control: no-cache\r\n"
    };
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{cache}Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}

fn handle_sse(mut stream: TcpStream, state: Arc<Mutex<SharedState>>) -> io::Result<()> {
    // No `Content-Length` or `Transfer-Encoding`: this is a long-lived
    // response that ends when the connection closes. HTTP/1.0-style "body
    // until EOF" framing keeps the write path trivial and lets each event
    // hit the wire after a single `flush()` with no chunked-encoding
    // accumulator getting in the way.
    let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nX-Accel-Buffering: no\r\n\r\n";
    stream.write_all(headers.as_bytes())?;
    stream.flush()?;

    let (tx, rx) = mpsc::channel::<String>();
    {
        let mut guard = state.lock().unwrap();
        if let Some(event) = guard.last_event.as_ref() {
            let _ = tx.send(event.clone());
        }
        guard.subscribers.push(tx);
    }

    while let Ok(event) = rx.recv() {
        if stream.write_all(event.as_bytes()).is_err() {
            break;
        }
        if stream.flush().is_err() {
            break;
        }
    }
    Ok(())
}

fn format_event(event: &str, source: &str) -> String {
    let payload = json_string(source);
    format!("event: {event}\ndata: {{\"source\":{payload}}}\n\n")
}

/// Minimal JSON string encoder so we can avoid pulling in `serde_json` for
/// the SSE payload. Only escapes what JSON strictly requires.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_escapes_required_chars() {
        assert_eq!(json_string("hello"), "\"hello\"");
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(json_string("line1\nline2"), "\"line1\\nline2\"");
        assert_eq!(json_string("tab\there"), "\"tab\\there\"");
        assert_eq!(json_string("ctl\x01"), "\"ctl\\u0001\"");
    }

    #[test]
    fn format_event_shape() {
        let ev = format_event("mermaid", "graph TD\nA-->B");
        assert!(ev.starts_with("event: mermaid\n"));
        assert!(ev.contains("\"source\":\"graph TD\\nA-->B\""));
        assert!(ev.ends_with("\n\n"));
    }

    #[test]
    fn handle_deduplicates_identical_sources() {
        let state = Arc::new(Mutex::new(SharedState::default()));
        let (tx, rx) = mpsc::channel::<String>();
        state.lock().unwrap().subscribers.push(tx);
        let mut handle = PreviewHandle {
            url: "http://127.0.0.1:0".into(),
            state: Arc::clone(&state),
            last_pushed: None,
        };
        handle.set_source(Some("graph TD\nA-->B"));
        handle.set_source(Some("graph TD\nA-->B")); // dedup
        handle.set_source(Some("graph LR\nC-->D"));
        let first = rx.recv().unwrap();
        assert!(first.contains("A-->B"));
        let second = rx.recv().unwrap();
        assert!(second.contains("C-->D"));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn handle_set_source_none_is_noop() {
        let state = Arc::new(Mutex::new(SharedState::default()));
        let (tx, rx) = mpsc::channel::<String>();
        state.lock().unwrap().subscribers.push(tx);
        let mut handle = PreviewHandle {
            url: "http://127.0.0.1:0".into(),
            state,
            last_pushed: None,
        };
        handle.set_source(None);
        assert!(rx.try_recv().is_err());
    }
}
