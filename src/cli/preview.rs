//! Embedded HTTP server that streams the current mermaid block to a browser
//! tab over Server-Sent Events. See issue #35.
//!
//! Lifecycle:
//! - `start()` binds `127.0.0.1:port` (or an OS-assigned port when None),
//!   spawns a server thread, and returns a [`PreviewHandle`] carrying the URL
//!   and a sender for source updates.
//! - The TUI calls [`PreviewHandle::set_source`] each frame; identical updates
//!   are dropped at the handle level (single-frame hash compare), so callers
//!   don't need to memoise.
//! - SSE clients are kept open in dedicated threads. Dropping the handle
//!   closes the listener; spawned client threads end when their socket dies.

use std::io::{self, Read};
use std::net::Ipv4Addr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use tiny_http::{Header, Method, Response, Server, StatusCode};

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
    let addr = (Ipv4Addr::LOCALHOST, port.unwrap_or(0));
    let server = Server::http(addr).map_err(|e| io::Error::other(e.to_string()))?;
    let bound = match server.server_addr() {
        tiny_http::ListenAddr::IP(addr) => addr,
        _ => return Err(io::Error::other("unexpected listen address")),
    };
    let url = format!("http://{}:{}", bound.ip(), bound.port());
    let state = Arc::new(Mutex::new(SharedState::default()));
    let server_state = Arc::clone(&state);
    thread::Builder::new()
        .name("preview-server".into())
        .spawn(move || run_server(server, server_state))?;
    Ok(PreviewHandle {
        url,
        state,
        last_pushed: None,
    })
}

fn run_server(server: Server, state: Arc<Mutex<SharedState>>) {
    for request in server.incoming_requests() {
        if request.method() != &Method::Get {
            let _ = request.respond(
                Response::from_string("method not allowed").with_status_code(StatusCode(405)),
            );
            continue;
        }
        match request.url() {
            "/" | "/index.html" => {
                let resp = Response::from_string(INDEX_HTML).with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                        .unwrap(),
                );
                let _ = request.respond(resp);
            }
            "/mermaid.min.js" => {
                let resp = Response::from_data(MERMAID_JS)
                    .with_header(
                        Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/javascript; charset=utf-8"[..],
                        )
                        .unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(
                            &b"Cache-Control"[..],
                            &b"public, max-age=31536000, immutable"[..],
                        )
                        .unwrap(),
                    );
                let _ = request.respond(resp);
            }
            "/events" => {
                let (tx, rx) = mpsc::channel::<String>();
                {
                    let mut guard = state.lock().unwrap();
                    if let Some(event) = guard.last_event.as_ref() {
                        let _ = tx.send(event.clone());
                    }
                    guard.subscribers.push(tx);
                }
                let headers = vec![
                    Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream"[..]).unwrap(),
                    Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap(),
                    Header::from_bytes(&b"X-Accel-Buffering"[..], &b"no"[..]).unwrap(),
                ];
                let resp = Response::new(StatusCode(200), headers, SseReader::new(rx), None, None);
                // SSE responses are long-lived; hand each one off to its own
                // thread so the recv() loop stays responsive.
                thread::spawn(move || {
                    let _ = request.respond(resp);
                });
            }
            _ => {
                let _ = request
                    .respond(Response::from_string("not found").with_status_code(StatusCode(404)));
            }
        }
    }
}

struct SseReader {
    rx: Receiver<String>,
    pending: Vec<u8>,
    pos: usize,
}

impl SseReader {
    fn new(rx: Receiver<String>) -> Self {
        Self {
            rx,
            pending: Vec::new(),
            pos: 0,
        }
    }
}

impl Read for SseReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.pending.len() {
            match self.rx.recv() {
                Ok(s) => {
                    self.pending = s.into_bytes();
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = std::cmp::min(buf.len(), self.pending.len() - self.pos);
        buf[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
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
        // Should have received exactly 2 events.
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
