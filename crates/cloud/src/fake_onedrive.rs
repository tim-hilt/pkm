//! A fake of Microsoft's OAuth and Graph APIs on a local HTTP port, for tests:
//! answers like the real ones for what the app uses, keeps the app folder in
//! memory, and can be made to revoke access, rotate refresh tokens, run out of
//! space or belong to a work account.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use pkm_base::json::Value;

use crate::fake_http::{Answer, FakeServer, Request, parse_query};
use crate::onedrive::{Hosts, REDIRECT_URI};
use crate::percent::{decode, encode};
use pkm_base::base64;
use pkm_base::sha256::sha256;

pub(crate) const CLIENT_ID: &str = "client-id";
pub(crate) const EMAIL: &str = "tim@example.com";
const APP_ROOT: &str = "/v1.0/me/drive/special/approot";

#[derive(Clone, Debug)]
pub(crate) struct File {
    pub data: Vec<u8>,
    pub version: u32,
}

/// An upload session waiting for its parts.
struct Upload {
    path: String,
    received: Vec<u8>,
}

struct State {
    next_id: u32,
    /// The blobs, by name below the app root.
    files: BTreeMap<String, File>,
    uploads: HashMap<String, Upload>,
    /// Code → the PKCE challenge it was issued for.
    codes: HashMap<String, String>,
    refresh_tokens: Vec<String>,
    access_tokens: Vec<String>,
    rotate: bool,
    full: bool,
    drive_type: &'static str,
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

pub(crate) struct FakeMicrosoft {
    server: FakeServer,
    state: Arc<Mutex<State>>,
}

fn quote(text: &str) -> String {
    Value::String(text.to_owned()).to_json()
}

fn failure(status: u16, code: &str) -> Answer {
    Answer::json(
        status,
        &format!(r#"{{"error":{{"code":{}}}}}"#, quote(code)),
    )
}

impl FakeMicrosoft {
    pub fn start() -> Self {
        let state = Arc::new(Mutex::new(State {
            next_id: 0,
            files: BTreeMap::new(),
            uploads: HashMap::new(),
            codes: HashMap::new(),
            refresh_tokens: Vec::new(),
            access_tokens: Vec::new(),
            rotate: false,
            full: false,
            drive_type: "personal",
            refreshes: 0,
            requests: Vec::new(),
        }));
        let shared = Arc::clone(&state);
        let server = FakeServer::start(move |request, base| {
            answer(request, base, &mut shared.lock().unwrap())
        });
        Self { server, state }
    }

    pub fn hosts(&self) -> Hosts {
        Hosts::for_test(&format!("http://{}", self.server.addr))
    }

    /// What the user's browser does with the sign-in page at `url`: consents
    /// or refuses, and ends at the redirect URL.
    pub fn browser(&self, url: &str, consent: bool) -> String {
        let (_, query) = url.split_once('?').unwrap();
        let params = parse_query(query);
        assert_eq!(params["client_id"], CLIENT_ID);
        assert_eq!(params["redirect_uri"], REDIRECT_URI);
        assert_eq!(params["scope"], "Files.ReadWrite.AppFolder offline_access");
        assert_eq!(params["response_type"], "code");
        assert_eq!(params["code_challenge_method"], "S256");
        let state = encode(&params["state"]);
        if !consent {
            return format!(
                "{REDIRECT_URI}?error=access_denied&error_description=The+user+has+denied+access&state={state}"
            );
        }
        let mut fake = self.state.lock().unwrap();
        let code = fake.id("code-");
        fake.codes
            .insert(code.clone(), params["code_challenge"].clone());
        format!("{REDIRECT_URI}?code={code}&state={state}")
    }

    /// Microsoft forgets the refresh tokens: access was revoked.
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

    /// Nothing can be created or changed any more: `quotaLimitReached`.
    pub fn fill_up(&self) {
        self.state.lock().unwrap().full = true;
    }

    /// The account is a work or school account.
    pub fn make_work_account(&self) {
        self.state.lock().unwrap().drive_type = "business";
    }

    pub fn refreshes(&self) -> u32 {
        self.state.lock().unwrap().refreshes
    }

    pub fn files(&self) -> BTreeMap<String, File> {
        self.state.lock().unwrap().files.clone()
    }

    /// The requests made, as `METHOD path`.
    pub fn requests(&self) -> Vec<String> {
        self.state.lock().unwrap().requests.clone()
    }

    /// Puts a file in the app folder as if the user had.
    pub fn add_file(&self, name: &str, data: &[u8]) {
        let file = File {
            data: data.to_vec(),
            version: 1,
        };
        self.state.lock().unwrap().files.insert(name.into(), file);
    }
}

fn answer(request: &Request, base: &str, fake: &mut State) -> Answer {
    fake.requests
        .push(format!("{} {}", request.method, request.path));
    if request.path == "/token" {
        return token(request, fake);
    }
    // Upload sessions and download URLs are authorized by their URL, and
    // refuse a token on top.
    let public = request.path.starts_with("/session/") || request.path.starts_with("/download/");
    let bearer = request
        .headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "));
    let authorized = if public {
        bearer.is_none()
    } else {
        bearer.is_some_and(|t| fake.access_tokens.iter().any(|a| a == t))
    };
    if !authorized {
        return failure(401, "InvalidAuthenticationToken");
    }
    if let Some(id) = request.path.strip_prefix("/session/") {
        return part(id, request, fake);
    }
    if let Some(name) = request.path.strip_prefix("/download/") {
        return match fake.files.get(&decode(name)) {
            Some(file) => Answer::data(200, file.data.clone()),
            None => failure(404, "itemNotFound"),
        };
    }
    graph(request, base, fake)
}

fn token(request: &Request, fake: &mut State) -> Answer {
    let form = parse_query(&String::from_utf8_lossy(&request.body));
    assert_eq!(form["client_id"], CLIENT_ID);
    let rejected = || {
        Answer::json(
            400,
            r#"{"error":"invalid_grant","error_description":"AADSTS70000"}"#,
        )
    };
    let mut refresh_token = None;
    match form["grant_type"].as_str() {
        "authorization_code" => {
            let Some(challenge) = fake.codes.remove(&form["code"]) else {
                return rejected();
            };
            let verifier = base64::encode_url(&sha256(form["code_verifier"].as_bytes()));
            if verifier != challenge || form["redirect_uri"] != REDIRECT_URI {
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
            // Microsoft hands out a new refresh token with most refreshes.
            if fake.rotate {
                fake.refresh_tokens.retain(|t| t != old);
                let issued = fake.id("refresh-");
                fake.refresh_tokens.push(issued.clone());
                refresh_token = Some(issued);
            }
        }
        _ => return Answer::json(400, r#"{"error":"unsupported_grant_type"}"#),
    }
    fake.refreshes += 1;
    let access = fake.id("access-");
    fake.access_tokens.push(access.clone());
    let refresh = refresh_token.map_or(String::new(), |t| {
        format!(r#","refresh_token":{}"#, quote(&t))
    });
    Answer::json(
        200,
        &format!(
            r#"{{"access_token":{},"expires_in":3599,"token_type":"Bearer","scope":"Files.ReadWrite.AppFolder"{refresh}}}"#,
            quote(&access)
        ),
    )
}

/// A finished upload: 201 if the blob is new.
fn store(fake: &mut State, name: &str, data: Vec<u8>) -> Answer {
    let version = fake.files.get(name).map_or(0, |f| f.version) + 1;
    let created = fake
        .files
        .insert(name.to_owned(), File { data, version })
        .is_none();
    Answer::json(if created { 201 } else { 200 }, "{}")
}

fn part(id: &str, request: &Request, fake: &mut State) -> Answer {
    let Some(upload) = fake.uploads.get_mut(id) else {
        return failure(404, "itemNotFound");
    };
    let range = request.headers["content-range"].strip_prefix("bytes ");
    let (span, total) = range.unwrap().split_once('/').unwrap();
    let start: usize = span.split('-').next().unwrap().parse().unwrap();
    // Graph wants the size of the whole with every part.
    let total: usize = total.parse().unwrap();
    if start != upload.received.len() {
        return failure(416, "invalidRange");
    }
    upload.received.extend_from_slice(&request.body);
    if upload.received.len() < total {
        return Answer::json(202, r#"{"nextExpectedRanges":[]}"#);
    }
    let upload = fake.uploads.remove(id).unwrap();
    store(fake, &upload.path, upload.received)
}

fn graph(request: &Request, base: &str, fake: &mut State) -> Answer {
    let Some(rest) = request.path.strip_prefix(APP_ROOT) else {
        return Answer::data(404, Vec::new());
    };
    // `:/name:/content`, or the app root itself.
    let (name, action) = match rest.strip_prefix(":/") {
        Some(rest) => {
            let (name, action) = rest.split_once(':').unwrap();
            (decode(name), action)
        }
        None => (String::new(), rest),
    };
    let method = request.method.as_str();
    let changes = matches!(
        (method, action),
        ("PUT", "/content") | ("POST", "/createUploadSession")
    );
    if fake.full && changes {
        return failure(507, "quotaLimitReached");
    }
    match (method, action) {
        ("GET", "") if name.is_empty() => {
            let created =
                format!(r#","createdBy":{{"user":{{"email":"{EMAIL}","displayName":"Tim"}}}}"#);
            Answer::json(
                200,
                &format!(
                    r#"{{"parentReference":{{"driveType":"{}"}}{created}}}"#,
                    fake.drive_type
                ),
            )
        }
        ("GET", "/children") => children(request, base, fake),
        ("GET", "/content") => match fake.files.contains_key(&name) {
            true => {
                let mut redirect = Answer::data(302, Vec::new());
                let url = format!("{base}/download/{}", encode(&name));
                redirect.headers.push(("Location", url));
                redirect
            }
            false => failure(404, "itemNotFound"),
        },
        ("PUT", "/content") => store(fake, &name, request.body.clone()),
        ("POST", "/createUploadSession") => {
            let id = fake.id("session");
            fake.uploads.insert(
                id.clone(),
                Upload {
                    path: name,
                    received: Vec::new(),
                },
            );
            Answer::json(200, &format!(r#"{{"uploadUrl":"{base}/session/{id}"}}"#))
        }
        ("DELETE", "") => match fake.files.remove(&name) {
            Some(_) => Answer::data(204, Vec::new()),
            None => failure(404, "itemNotFound"),
        },
        _ => Answer::json(405, "{}"),
    }
}

/// The blobs, two to a page, the link to the next page carrying the offset.
fn children(request: &Request, base: &str, fake: &State) -> Answer {
    const PAGE: usize = 2;
    let start: usize = request
        .query
        .get("$skiptoken")
        .and_then(|t| t.parse().ok())
        .unwrap_or(0);
    let items: Vec<String> = fake
        .files
        .iter()
        .skip(start)
        .take(PAGE)
        .map(|(name, file)| {
            format!(
                r#"{{"name":{},"size":{},"eTag":"\"v{}\"","file":{{}}}}"#,
                quote(name),
                file.data.len(),
                file.version
            )
        })
        .collect();
    let next = if start + PAGE < fake.files.len() {
        format!(
            r#","@odata.nextLink":"{base}{APP_ROOT}/children?$skiptoken={}""#,
            start + PAGE
        )
    } else {
        String::new()
    };
    Answer::json(200, &format!(r#"{{"value":[{}]{next}}}"#, items.join(",")))
}
