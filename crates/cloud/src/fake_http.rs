//! A local HTTP server for the fakes of the cloud providers' APIs: parses a
//! request, asks the fake for the answer and writes it back.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::thread;

use crate::percent::decode;

pub(crate) struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub(crate) struct Answer {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Answer {
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn data(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
        }
    }
}

/// Serves `answer` on a local port until dropped, one thread per connection.
/// The closure is told the server's base URL too.
pub(crate) struct FakeServer {
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl FakeServer {
    pub fn start(answer: impl Fn(&Request, &str) -> Answer + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let answer = Arc::new(answer);
        let base = format!("http://{addr}");
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Relaxed) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                let (answer, base) = (Arc::clone(&answer), base.clone());
                thread::spawn(move || serve(stream, &*answer, &base));
            }
        });
        Self { addr, stop }
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        let _ = TcpStream::connect(self.addr);
    }
}

pub(crate) fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(&v.replace('+', " "))))
        .collect()
}

fn serve(mut stream: TcpStream, answer: &dyn Fn(&Request, &str) -> Answer, base: &str) {
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    let Answer {
        status,
        headers,
        body,
    } = answer(&request, base);
    let mut head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
}

fn read_more(stream: &mut TcpStream, into: &mut Vec<u8>) -> Option<()> {
    let mut chunk = [0u8; 4096];
    let n = stream.read(&mut chunk).ok()?;
    into.extend_from_slice(&chunk[..n]);
    (n > 0).then_some(())
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut first = lines.next()?.split(' ');
    let (method, target) = (first.next()?.to_owned(), first.next()?);
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let mut rest = buffer[head_end + 4..].to_vec();
    let body = if headers
        .get("transfer-encoding")
        .is_some_and(|v| v == "chunked")
    {
        // Chunks: a hex size line, the data, CRLF; a size of 0 ends it.
        let mut body = Vec::new();
        loop {
            let line_end = loop {
                if let Some(i) = rest.windows(2).position(|w| w == b"\r\n") {
                    break i;
                }
                read_more(stream, &mut rest)?;
            };
            let size =
                usize::from_str_radix(std::str::from_utf8(&rest[..line_end]).ok()?.trim(), 16)
                    .ok()?;
            while rest.len() < line_end + 2 + size + 2 {
                read_more(stream, &mut rest)?;
            }
            body.extend_from_slice(&rest[line_end + 2..line_end + 2 + size]);
            rest.drain(..line_end + 2 + size + 2);
            if size == 0 {
                break body;
            }
        }
    } else {
        let length: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        while rest.len() < length {
            read_more(stream, &mut rest)?;
        }
        rest
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    Some(Request {
        method,
        path: path.to_owned(),
        query: parse_query(query),
        headers,
        body,
    })
}
