//! A fake of Google's OAuth and Drive APIs on a local HTTP port, for tests:
//! answers like the real ones for what the app uses, keeps files in memory,
//! and can be made to revoke access, rotate refresh tokens or run out of space.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::thread;

use base64::prelude::{BASE64_URL_SAFE_NO_PAD, Engine};
use excalidraw_native_core::json::{self, Value};

use crate::gdrive::Hosts;
use crate::percent::{decode, encode};
use crate::sha256::sha256;

pub(crate) const CLIENT_ID: &str = "123-abc.apps.googleusercontent.com";
pub(crate) const REDIRECT: &str = "com.googleusercontent.apps.123-abc:/oauth2redirect";
pub(crate) const EMAIL: &str = "tim@example.com";

#[derive(Clone, Debug)]
pub(crate) struct File {
    pub id: String,
    pub name: String,
    pub parent: String,
    pub mime: String,
    pub data: Vec<u8>,
    pub version: u32,
}

#[derive(Default)]
struct State {
    next_id: u32,
    files: Vec<File>,
    /// Code → the PKCE challenge it was issued for.
    codes: HashMap<String, String>,
    refresh_tokens: Vec<String>,
    access_tokens: Vec<String>,
    rotate: bool,
    full: bool,
    /// How many access tokens were handed out.
    refreshes: u32,
    /// Every request made, as `METHOD path`.
    requests: Vec<String>,
}

impl State {
    fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}{}", self.next_id)
    }
}

pub(crate) struct FakeGoogle {
    addr: SocketAddr,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
}

struct Request {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

struct Answer(u16, Vec<u8>);

fn json_answer(status: u16, body: &str) -> Answer {
    Answer(status, body.as_bytes().to_vec())
}

fn quote(text: &str) -> String {
    Value::String(text.to_owned()).to_json()
}

impl FakeGoogle {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, stopping) = (Arc::clone(&state), Arc::clone(&stop));
        thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Relaxed) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                let shared = Arc::clone(&shared);
                thread::spawn(move || serve(stream, &shared));
            }
        });
        Self { addr, state, stop }
    }

    pub fn hosts(&self) -> Hosts {
        let base = format!("http://{}", self.addr);
        Hosts::for_test(&base)
    }

    /// What the user's browser does with the sign-in page at `url`: consents
    /// or refuses, and ends at the redirect URL.
    pub fn browser(&self, url: &str, consent: bool) -> String {
        let (_, query) = url.split_once('?').unwrap();
        let params = parse_query(query);
        assert_eq!(params["client_id"], CLIENT_ID);
        assert_eq!(params["redirect_uri"], REDIRECT);
        assert_eq!(
            params["scope"],
            "https://www.googleapis.com/auth/drive.file"
        );
        assert_eq!(params["response_type"], "code");
        assert_eq!(params["code_challenge_method"], "S256");
        let state = encode(&params["state"]);
        if !consent {
            return format!("{REDIRECT}?error=access_denied&state={state}");
        }
        let mut fake = self.state.lock().unwrap();
        let code = fake.id("code-");
        fake.codes
            .insert(code.clone(), params["code_challenge"].clone());
        format!("{REDIRECT}?code={code}&state={state}")
    }

    /// Google forgets the refresh tokens: access was revoked.
    pub fn revoke(&self) {
        let mut fake = self.state.lock().unwrap();
        fake.refresh_tokens.clear();
        fake.access_tokens.clear();
    }

    /// The access tokens handed out so far stop working.
    pub fn expire_access_tokens(&self) {
        self.state.lock().unwrap().access_tokens.clear();
    }

    /// Every refresh answers with a new refresh token too.
    pub fn rotate_refresh_tokens(&self) {
        self.state.lock().unwrap().rotate = true;
    }

    /// Uploads fail with `storageQuotaExceeded`.
    pub fn fill_up(&self) {
        self.state.lock().unwrap().full = true;
    }

    pub fn refreshes(&self) -> u32 {
        self.state.lock().unwrap().refreshes
    }

    pub fn files(&self) -> Vec<File> {
        self.state.lock().unwrap().files.clone()
    }

    /// The requests made, as `METHOD path`.
    pub fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }

    /// Everything in Drive is gone.
    pub fn remove_all(&self) {
        self.state.lock().unwrap().files.clear();
    }

    /// Puts a file in `parent` as if the user had.
    pub fn add_file(&self, name: &str, parent: &str, data: &[u8]) {
        let mut fake = self.state.lock().unwrap();
        let id = fake.id("file-");
        fake.files.push(File {
            id,
            name: name.into(),
            parent: parent.into(),
            mime: "application/json".into(),
            data: data.to_vec(),
            version: 1,
        });
    }
}

impl Drop for FakeGoogle {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        let _ = TcpStream::connect(self.addr);
    }
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(&v.replace('+', " "))))
        .collect()
}

fn serve(mut stream: TcpStream, state: &Mutex<State>) {
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    let Answer(status, body) = answer(&request, &mut state.lock().unwrap());
    let head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
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

fn answer(request: &Request, fake: &mut State) -> Answer {
    fake.requests
        .push(format!("{} {}", request.method, request.path));
    if request.path == "/token" {
        return token(request, fake);
    }
    let bearer = request
        .headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "));
    if !bearer.is_some_and(|t| fake.access_tokens.iter().any(|a| a == t)) {
        return json_answer(
            401,
            r#"{"error":{"code":401,"message":"Invalid Credentials"}}"#,
        );
    }
    drive(request, fake)
}

fn token(request: &Request, fake: &mut State) -> Answer {
    let form = parse_query(&String::from_utf8_lossy(&request.body));
    assert_eq!(form["client_id"], CLIENT_ID);
    let rejected = || json_answer(400, r#"{"error":"invalid_grant"}"#);
    let mut refresh_token = None;
    match form["grant_type"].as_str() {
        "authorization_code" => {
            let Some(challenge) = fake.codes.remove(&form["code"]) else {
                return rejected();
            };
            // PKCE: the verifier must be the one the challenge was made from.
            let verifier = BASE64_URL_SAFE_NO_PAD.encode(sha256(form["code_verifier"].as_bytes()));
            if verifier != challenge || form["redirect_uri"] != REDIRECT {
                return rejected();
            }
            let issued = fake.id("refresh-");
            fake.refresh_tokens.push(issued.clone());
            refresh_token = Some(issued);
        }
        "refresh_token" => {
            let old = &form["refresh_token"];
            if !fake.refresh_tokens.contains(old) {
                return rejected();
            }
            if fake.rotate {
                fake.refresh_tokens.retain(|t| t != old);
                let issued = fake.id("refresh-");
                fake.refresh_tokens.push(issued.clone());
                refresh_token = Some(issued);
            }
        }
        _ => return json_answer(400, r#"{"error":"unsupported_grant_type"}"#),
    }
    fake.refreshes += 1;
    let access = fake.id("access-");
    fake.access_tokens.push(access.clone());
    let refresh = refresh_token.map_or(String::new(), |t| {
        format!(r#","refresh_token":{}"#, quote(&t))
    });
    json_answer(
        200,
        &format!(
            r#"{{"access_token":{},"expires_in":3599,"token_type":"Bearer"{refresh}}}"#,
            quote(&access)
        ),
    )
}

/// The parts of a Drive query the app uses.
struct Query {
    name: Option<String>,
    mime: Option<String>,
    parent: String,
}

fn parse_drive_query(q: &str) -> Query {
    let field = |key: &str| {
        let rest = q.split(&format!("{key} = '")).nth(1)?;
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => out.extend(chars.next()),
                '\'' => return Some(out),
                c => out.push(c),
            }
        }
        None
    };
    let parent = q
        .split(" in parents")
        .next()
        .unwrap()
        .rsplit('\'')
        .nth(1)
        .unwrap();
    Query {
        name: field("name"),
        mime: field("mimeType"),
        parent: parent.to_owned(),
    }
}

fn meta(file: &File) -> String {
    format!(
        r#"{{"id":{},"name":{},"version":"{}"}}"#,
        quote(&file.id),
        quote(&file.name),
        file.version
    )
}

fn drive(request: &Request, fake: &mut State) -> Answer {
    let path = request.path.as_str();
    let not_found = || json_answer(404, r#"{"error":{"code":404,"message":"File not found"}}"#);
    match (request.method.as_str(), path) {
        ("GET", "/drive/v3/about") => {
            json_answer(200, &format!(r#"{{"user":{{"emailAddress":"{EMAIL}"}}}}"#))
        }
        ("GET", "/drive/v3/files") => {
            let query = parse_drive_query(&request.query["q"]);
            let page = request.query["pageSize"].parse::<usize>().unwrap();
            let matching: Vec<&File> = fake
                .files
                .iter()
                .filter(|f| f.parent == query.parent)
                .filter(|f| query.name.as_ref().is_none_or(|n| &f.name == n))
                .filter(|f| query.mime.as_ref().is_none_or(|m| &f.mime == m))
                .collect();
            // Pages of `page` files, the token being the offset.
            let from: usize = request
                .query
                .get("pageToken")
                .map_or(0, |t| t.parse().unwrap());
            let shown: Vec<String> = matching
                .iter()
                .skip(from)
                .take(page)
                .map(|f| meta(f))
                .collect();
            let next = if from + page < matching.len() {
                format!(r#","nextPageToken":"{}""#, from + page)
            } else {
                String::new()
            };
            json_answer(200, &format!(r#"{{"files":[{}]{next}}}"#, shown.join(",")))
        }
        ("POST", "/drive/v3/files") => {
            let value = json::parse(&String::from_utf8_lossy(&request.body)).unwrap();
            let id = fake.id("folder-");
            fake.files.push(File {
                id: id.clone(),
                name: value.get("name").unwrap().as_str().unwrap().into(),
                parent: "root".into(),
                mime: value.get("mimeType").unwrap().as_str().unwrap().into(),
                data: Vec::new(),
                version: 1,
            });
            json_answer(200, &format!(r#"{{"id":{}}}"#, quote(&id)))
        }
        ("POST", "/upload/drive/v3/files") => {
            if fake.full {
                return quota_exceeded();
            }
            // multipart/related: the metadata, then the content.
            let content_type = &request.headers["content-type"];
            let boundary = format!("--{}", content_type.split("boundary=").nth(1).unwrap());
            let body = String::from_utf8_lossy(&request.body).into_owned();
            let parts: Vec<&str> = body.split(boundary.as_str()).collect();
            let part = |i: usize| {
                parts[i]
                    .split_once("\r\n\r\n")
                    .unwrap()
                    .1
                    .trim_end_matches("\r\n")
            };
            let value = json::parse(part(1)).unwrap();
            let parent = value.get("parents").unwrap().as_array().unwrap()[0]
                .as_str()
                .unwrap();
            if !fake.files.iter().any(|f| f.id == parent) {
                return not_found();
            }
            let id = fake.id("file-");
            fake.files.push(File {
                id: id.clone(),
                name: value.get("name").unwrap().as_str().unwrap().into(),
                parent: parent.into(),
                mime: value.get("mimeType").unwrap().as_str().unwrap().into(),
                data: part(2).as_bytes().to_vec(),
                version: 1,
            });
            json_answer(200, &format!(r#"{{"id":{}}}"#, quote(&id)))
        }
        (method, _) => {
            let id = path.rsplit('/').next().unwrap();
            let Some(i) = fake.files.iter().position(|f| f.id == id) else {
                return not_found();
            };
            match method {
                "GET" => Answer(200, fake.files[i].data.clone()),
                "PATCH" => {
                    if fake.full {
                        return quota_exceeded();
                    }
                    let file = &mut fake.files[i];
                    file.data = request.body.clone();
                    file.version += 1;
                    json_answer(200, &meta(file))
                }
                "DELETE" => {
                    fake.files.remove(i);
                    Answer(204, Vec::new())
                }
                _ => json_answer(405, "{}"),
            }
        }
    }
}

fn quota_exceeded() -> Answer {
    json_answer(
        403,
        r#"{"error":{"code":403,"message":"The user's Drive storage quota has been exceeded.","errors":[{"reason":"storageQuotaExceeded"}]}}"#,
    )
}
