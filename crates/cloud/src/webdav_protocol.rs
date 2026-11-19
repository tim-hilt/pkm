//! The WebDAV protocol (ADR-0012, ADR-0016), without any sending: which
//! request to send for connect, list, get, put and delete, and what each
//! answer means. The native WebDAV storage sends with ureq, the web app with
//! `fetch`; both run the same `Exchange`.
//!
//! An `Exchange` is one operation: `start` gives its first request, `answer`
//! takes the status and body of the answer and gives the next request or the
//! result, and `failed` ends it when the request could not be sent at all
//! (no network, an untrusted certificate, CORS).
//!
//! Everything lives in the `pkm` folder inside the Server URL, one folder in
//! it per Context (`pkm/Drawings`).
//! Blobs are uploaded to a hidden temporary name and then moved into place, so
//! a half-written blob is never visible under its final name. Every request
//! carries `Authorization: Basic`, so no client ever waits for a challenge.

use crate::percent::{decode, encode};
use crate::storage::{Blob, ConnectError, Tree};
use pkm_base::base64;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io;

/// The folder in the Server URL that holds everything the app stores.
pub const FOLDER: &str = "pkm";

/// The folder where blobs are uploaded before they are moved into place. It
/// starts with a dot, so it is no valid blob name and never listed. It is not
/// the folder of the blobs: some servers (rclone) can't list a folder while a
/// file in it is being written.
const INCOMING: &str = ".incoming";

/// A request for a client to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

/// What the server answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpAnswer {
    pub status: u16,
    pub body: Vec<u8>,
}

/// What an operation does next.
#[derive(Debug, PartialEq, Eq)]
pub enum Turn<T> {
    Send(HttpRequest),
    Done(T),
}

/// One operation against the server, answered request by request.
pub trait Exchange {
    type Output;
    /// The first request.
    fn start(&mut self) -> HttpRequest;
    /// The next request, or the result, given the answer to the last one.
    fn answer(&mut self, answer: HttpAnswer) -> Turn<Self::Output>;
    /// The result when the last request could not be sent.
    fn failed(&mut self, error: io::Error) -> Self::Output;
}

/// The requests of one server. Holds the credentials; contacts nobody.
#[derive(Clone)]
pub struct WebDav {
    /// The Server URL, without a trailing slash.
    base: String,
    /// The URL of the `pkm` folder inside it.
    parent: String,
    /// The URL of the Context's folder inside that.
    root: String,
    /// The `Authorization` header value.
    pub(crate) auth: String,
}

impl std::fmt::Debug for WebDav {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // No credentials.
        f.debug_struct("WebDav")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl WebDav {
    /// The protocol for `url`, which must be https, for the folder of
    /// `context` ("Drawings").
    pub fn new(
        url: &str,
        username: &str,
        password: &str,
        context: &str,
    ) -> Result<Self, ConnectError> {
        let url = url.trim();
        if !url
            .get(..8)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
        {
            return Err(ConnectError::NotHttps);
        }
        let base = url.trim_end_matches('/').to_owned();
        let parent = format!("{base}/{FOLDER}");
        Ok(Self {
            root: format!("{parent}/{}", encode(context)),
            parent,
            base,
            auth: basic_auth(username, password),
        })
    }

    /// Checks that the server is a WebDAV server that accepts the login, and
    /// that the Context's folder in `pkm` is there, creating them if not.
    pub fn connect(&self) -> impl Exchange<Output = Result<(), ConnectError>> + use<> {
        Connect {
            dav: self.clone(),
            phase: ConnectPhase::Server,
            folder: 0,
        }
    }

    /// Everything in the folder and below it: one `PROPFIND` of Depth 1 per
    /// directory.
    pub fn scan(&self) -> impl Exchange<Output = io::Result<Tree>> + use<> {
        Scan {
            dav: self.clone(),
            dir: String::new(),
            pending: Vec::new(),
            tree: Tree::default(),
        }
    }

    /// Makes the directory `path` in the folder, and the ones above it; fine
    /// if they exist. Also makes the folder and `pkm` if they are missing.
    pub fn create_dir(&self, path: &str) -> impl Exchange<Output = io::Result<()>> + use<> {
        let mut urls = vec![self.parent.clone(), self.root.clone()];
        urls.extend(self.ancestors(path, true));
        MakeFolders {
            dav: self.clone(),
            urls,
            next: 0,
        }
    }

    /// The content of the blob `name`; none if there is no such blob.
    pub fn get(&self, name: &str) -> impl Exchange<Output = io::Result<Option<Vec<u8>>>> + use<> {
        Single {
            request: Some(self.request("GET", self.url(name), &[], Vec::new())),
            read: |answer: HttpAnswer| {
                refuse(answer.status)?;
                match answer.status {
                    200..=299 => Ok(Some(answer.body)),
                    404 => Ok(None),
                    status => Err(unexpected("GET", status)),
                }
            },
        }
    }

    /// Deletes the blob `name`; fine if there is none.
    pub fn delete(&self, name: &str) -> impl Exchange<Output = io::Result<()>> + use<> {
        Single {
            request: Some(self.request("DELETE", self.url(name), &[], Vec::new())),
            read: |answer: HttpAnswer| {
                refuse(answer.status)?;
                match answer.status {
                    200..=299 | 404 => Ok(()),
                    status => Err(unexpected("DELETE", status)),
                }
            },
        }
    }

    /// Stores `data` as the blob `name`: uploads it to `temp`, a name no other
    /// upload uses, in the `.incoming` folder, then moves it over `name`.
    /// `incoming_exists` says the folder is known to be there; if not it is
    /// created first, with the Context's folder and `pkm` if they are missing
    /// too: a Context added to the app after connecting has no folder yet. A
    /// failed upload deletes what it left at `temp`.
    pub fn put(
        &self,
        name: &str,
        temp: &str,
        data: Vec<u8>,
        incoming_exists: bool,
    ) -> impl Exchange<Output = io::Result<()>> + use<> {
        let incoming = format!("{}/{INCOMING}", self.root);
        Put {
            temp: format!("{incoming}/{}", encode(temp)),
            dav: self.clone(),
            destination: self.url(name),
            parents: self.ancestors(name, false),
            parents_made: false,
            incoming,
            data,
            phase: if incoming_exists {
                PutPhase::Upload
            } else {
                PutPhase::MakeFolder {
                    folder: INCOMING_FOLDER,
                    made: false,
                }
            },
        }
    }

    /// The URL of the blob or directory `path` in the folder.
    fn url(&self, path: &str) -> String {
        let encoded: Vec<_> = path.split('/').map(encode).collect();
        format!("{}/{}", self.root, encoded.join("/"))
    }

    /// The URLs of the directories above `path`, outermost first, and
    /// `path` itself if `own`.
    fn ancestors(&self, path: &str, own: bool) -> Vec<String> {
        let segments: Vec<_> = path.split('/').collect();
        let count = if own {
            segments.len()
        } else {
            segments.len() - 1
        };
        (1..=count)
            .map(|n| self.url(&segments[..n].join("/")))
            .collect()
    }

    fn request(
        &self,
        method: &'static str,
        url: String,
        headers: &[(&'static str, &str)],
        body: Vec<u8>,
    ) -> HttpRequest {
        let mut all = vec![("Authorization", self.auth.clone())];
        all.extend(headers.iter().map(|(n, v)| (*n, (*v).to_owned())));
        HttpRequest {
            method,
            url,
            headers: all,
            body,
        }
    }

    fn propfind(&self, url: &str, depth: &str) -> HttpRequest {
        const WANTED: &str = r#"<?xml version="1.0"?><propfind xmlns="DAV:"><prop><resourcetype/><getcontentlength/><getetag/><getlastmodified/></prop></propfind>"#;
        self.request(
            "PROPFIND",
            url.to_owned(),
            &[("Depth", depth), ("Content-Type", "application/xml")],
            WANTED.as_bytes().to_vec(),
        )
    }

    fn mkcol(&self, url: &str) -> HttpRequest {
        self.request("MKCOL", url.to_owned(), &[], Vec::new())
    }
}

/// An operation of one request.
struct Single<T> {
    request: Option<HttpRequest>,
    read: fn(HttpAnswer) -> T,
}

impl<T: FromIoError> Exchange for Single<T> {
    type Output = T;
    fn start(&mut self) -> HttpRequest {
        self.request.take().expect("started once")
    }
    fn answer(&mut self, answer: HttpAnswer) -> Turn<T> {
        Turn::Done((self.read)(answer))
    }
    fn failed(&mut self, error: io::Error) -> T {
        T::from_io_error(error)
    }
}

/// A result that can be made from a failure to send.
trait FromIoError {
    fn from_io_error(error: io::Error) -> Self;
}

impl<T> FromIoError for io::Result<T> {
    fn from_io_error(error: io::Error) -> Self {
        Err(error)
    }
}

enum ConnectPhase {
    Server,
    Folder,
    MakeFolder,
}

struct Connect {
    dav: WebDav,
    phase: ConnectPhase,
    /// Which of the folders to make sure of the phase is about.
    folder: usize,
}

impl Connect {
    /// Makes sure of the folders in turn: `pkm`, then the Context's inside it.
    fn folder_url(&self) -> &str {
        [&self.dav.parent, &self.dav.root][self.folder]
    }

    /// The folder is there: on to the next, or done.
    fn found(&mut self) -> Turn<Result<(), ConnectError>> {
        self.phase = ConnectPhase::Folder;
        self.folder += 1;
        if self.folder < 2 {
            Turn::Send(self.dav.propfind(self.folder_url(), "0"))
        } else {
            Turn::Done(Ok(()))
        }
    }
}

impl Exchange for Connect {
    type Output = Result<(), ConnectError>;

    fn start(&mut self) -> HttpRequest {
        // The URL itself must be a WebDAV collection.
        self.dav.propfind(&self.dav.base, "0")
    }

    fn answer(&mut self, answer: HttpAnswer) -> Turn<Self::Output> {
        let status = answer.status;
        let fail = |error| Turn::Done(Err(error));
        if status == 401 {
            return fail(ConnectError::WrongCredentials);
        }
        match self.phase {
            ConnectPhase::Server => match status {
                207 => {
                    // Its folders are there, or are created now.
                    self.phase = ConnectPhase::Folder;
                    Turn::Send(self.dav.propfind(self.folder_url(), "0"))
                }
                500..=599 => fail(ConnectError::Unreachable),
                _ => fail(ConnectError::NotWebDav),
            },
            ConnectPhase::Folder => match status {
                207 => self.found(),
                404 => {
                    self.phase = ConnectPhase::MakeFolder;
                    Turn::Send(self.dav.mkcol(self.folder_url()))
                }
                507 => fail(ConnectError::Unreachable),
                _ => fail(ConnectError::NotWebDav),
            },
            ConnectPhase::MakeFolder => match status {
                // 405: it exists already.
                200..=299 | 405 => self.found(),
                _ => fail(ConnectError::NotWebDav),
            },
        }
    }

    fn failed(&mut self, _: io::Error) -> Self::Output {
        Err(match self.phase {
            ConnectPhase::MakeFolder => ConnectError::NotWebDav,
            _ => ConnectError::Unreachable,
        })
    }
}

/// The folders a put may make, each in the one before: `pkm`, the
/// Context's folder and its `.incoming` folder.
const INCOMING_FOLDER: usize = 2;

enum PutPhase {
    /// Making the folder of that index; `made` says a folder was made, after
    /// which a missing parent (409) means it isn't coming back.
    MakeFolder {
        folder: usize,
        made: bool,
    },
    Upload,
    Move,
    /// Making the directory of that index of `parents`, which the blob's
    /// directories are missing.
    MakeParent(usize),
    /// Deleting the temporary blob after the error.
    Clean(io::Error),
}

struct Put {
    dav: WebDav,
    incoming: String,
    temp: String,
    destination: String,
    /// The URLs of the directories above the destination, outermost first.
    parents: Vec<String>,
    /// Whether they have been made: a second refusal is final.
    parents_made: bool,
    data: Vec<u8>,
    phase: PutPhase,
}

impl Put {
    fn make_folder(&mut self, folder: usize, made: bool) -> HttpRequest {
        self.phase = PutPhase::MakeFolder { folder, made };
        let url = [&self.dav.parent, &self.dav.root, &self.incoming][folder];
        self.dav.mkcol(url)
    }

    fn upload(&mut self) -> HttpRequest {
        self.phase = PutPhase::Upload;
        let data = std::mem::take(&mut self.data);
        self.dav.request("PUT", self.temp.clone(), &[], data)
    }

    fn move_into_place(&mut self) -> HttpRequest {
        self.phase = PutPhase::Move;
        let headers = [("Destination", &*self.destination), ("Overwrite", "T")];
        self.dav
            .request("MOVE", self.temp.clone(), &headers, Vec::new())
    }

    fn make_parent(&mut self, index: usize) -> HttpRequest {
        self.phase = PutPhase::MakeParent(index);
        self.dav.mkcol(&self.parents[index])
    }

    fn clean(&mut self, error: io::Error) -> Turn<io::Result<()>> {
        let delete = self
            .dav
            .request("DELETE", self.temp.clone(), &[], Vec::new());
        self.phase = PutPhase::Clean(error);
        Turn::Send(delete)
    }
}

impl Exchange for Put {
    type Output = io::Result<()>;

    fn start(&mut self) -> HttpRequest {
        match self.phase {
            PutPhase::MakeFolder { folder, made } => self.make_folder(folder, made),
            _ => self.upload(),
        }
    }

    fn answer(&mut self, answer: HttpAnswer) -> Turn<Self::Output> {
        let status = answer.status;
        let refused = refuse(status);
        match std::mem::replace(&mut self.phase, PutPhase::Upload) {
            PutPhase::MakeFolder { folder, made } => match (refused, status) {
                (Err(error), _) => Turn::Done(Err(error)),
                // 405: it exists already.
                (_, 200..=299 | 405) if folder == INCOMING_FOLDER => Turn::Send(self.upload()),
                (_, 200..=299 | 405) => Turn::Send(self.make_folder(folder + 1, true)),
                // 409: the folder above is missing.
                (_, 409) if folder > 0 && !made => Turn::Send(self.make_folder(folder - 1, false)),
                _ => Turn::Done(Err(unexpected("MKCOL", status))),
            },
            PutPhase::Upload => match (refused, status) {
                (Err(error), _) => self.clean(error),
                (_, 200..=299) => Turn::Send(self.move_into_place()),
                _ => self.clean(unexpected("PUT", status)),
            },
            PutPhase::Move => match (refused, status) {
                (Err(error), _) => self.clean(error),
                (_, 200..=299) => Turn::Done(Ok(())),
                // 409: the blob's directories are missing; some servers
                // (rclone) say 403.
                (_, 403 | 409) if !self.parents.is_empty() && !self.parents_made => {
                    self.parents_made = true;
                    Turn::Send(self.make_parent(0))
                }
                _ => self.clean(unexpected("MOVE", status)),
            },
            PutPhase::MakeParent(index) => match (refused, status) {
                (Err(error), _) => self.clean(error),
                // 405: it exists already.
                (_, 200..=299 | 405) if index + 1 < self.parents.len() => {
                    Turn::Send(self.make_parent(index + 1))
                }
                (_, 200..=299 | 405) => Turn::Send(self.move_into_place()),
                _ => self.clean(unexpected("MKCOL", status)),
            },
            PutPhase::Clean(error) => Turn::Done(Err(error)),
        }
    }

    fn failed(&mut self, error: io::Error) -> Self::Output {
        // The upload failed; so would the cleaning up. Report the first error.
        match std::mem::replace(&mut self.phase, PutPhase::Upload) {
            PutPhase::Clean(first) => Err(first),
            _ => Err(error),
        }
    }
}

/// Walks the folder, a directory per request.
struct Scan {
    dav: WebDav,
    /// The directory being listed, relative to the folder; empty for it.
    dir: String,
    /// The directories still to list.
    pending: Vec<String>,
    tree: Tree,
}

impl Scan {
    fn propfind_dir(&self) -> HttpRequest {
        let url = if self.dir.is_empty() {
            self.dav.root.clone()
        } else {
            self.dav.url(&self.dir)
        };
        self.dav.propfind(&url, "1")
    }
}

impl Exchange for Scan {
    type Output = io::Result<Tree>;

    fn start(&mut self) -> HttpRequest {
        self.propfind_dir()
    }

    fn answer(&mut self, answer: HttpAnswer) -> Turn<Self::Output> {
        let status = answer.status;
        if let Err(error) = refuse(status) {
            return Turn::Done(Err(error));
        }
        match status {
            207 => {
                let xml = String::from_utf8_lossy(&answer.body);
                let listed = url_path(&self.propfind_dir().url);
                let entries = match parse_listing(&xml, &listed) {
                    Ok(entries) => entries,
                    Err(error) => return Turn::Done(Err(io::Error::other(error))),
                };
                if entries.is_empty() && !self.dir.is_empty() {
                    self.tree.empty_dirs.push(self.dir.clone());
                }
                for child in entries {
                    let path = if self.dir.is_empty() {
                        child.name
                    } else {
                        format!("{}/{}", self.dir, child.name)
                    };
                    match child.kind {
                        ChildKind::File { marker } => {
                            self.tree.blobs.push(Blob { name: path, marker });
                        }
                        ChildKind::Dir => self.pending.push(path),
                    }
                }
            }
            // The folder is made with the first put; a directory deleted
            // meanwhile is gone.
            404 => {}
            _ => return Turn::Done(Err(unexpected("PROPFIND", status))),
        }
        match self.pending.pop() {
            Some(next) => {
                self.dir = next;
                Turn::Send(self.propfind_dir())
            }
            None => Turn::Done(Ok(std::mem::take(&mut self.tree))),
        }
    }

    fn failed(&mut self, error: io::Error) -> Self::Output {
        Err(error)
    }
}

/// Makes folders in turn, each in the one before.
struct MakeFolders {
    dav: WebDav,
    urls: Vec<String>,
    next: usize,
}

impl Exchange for MakeFolders {
    type Output = io::Result<()>;

    fn start(&mut self) -> HttpRequest {
        self.next = 1;
        self.dav.mkcol(&self.urls[0])
    }

    fn answer(&mut self, answer: HttpAnswer) -> Turn<Self::Output> {
        let status = answer.status;
        match (refuse(status), status) {
            (Err(error), _) => Turn::Done(Err(error)),
            // 405: it exists already.
            (_, 200..=299 | 405) => match self.urls.get(self.next) {
                Some(url) => {
                    self.next += 1;
                    Turn::Send(self.dav.mkcol(url))
                }
                None => Turn::Done(Ok(())),
            },
            _ => Turn::Done(Err(unexpected("MKCOL", status))),
        }
    }

    fn failed(&mut self, error: io::Error) -> Self::Output {
        Err(error)
    }
}

/// The errors every request shares: 401 is `PermissionDenied`, and 507
/// Insufficient Storage is `StorageFull`.
pub(crate) fn refuse(status: u16) -> io::Result<()> {
    match status {
        401 => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "401 Unauthorized",
        )),
        507 => Err(io::Error::new(
            io::ErrorKind::StorageFull,
            "507 Insufficient Storage",
        )),
        _ => Ok(()),
    }
}

fn unexpected(method: &str, status: u16) -> io::Error {
    io::Error::other(format!("{method} answered with status {status}"))
}

/// A file or directory in a `PROPFIND` answer.
pub(crate) struct Child {
    pub name: String,
    pub kind: ChildKind,
}

pub(crate) enum ChildKind {
    Dir,
    /// The marker is the ETag, or the modified time and size for a server
    /// without ETags.
    File {
        marker: String,
    },
}

/// The decoded path of `url`, which may be a path already, without a
/// trailing slash.
fn url_path(url: &str) -> String {
    let path = match url.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("", |slash| &rest[slash..]),
        None => url,
    };
    decode(path).trim_end_matches('/').to_owned()
}

/// What is in the directory at path `listed` according to a `PROPFIND` answer
/// of Depth 1: the children, but not the directory itself and no hidden ones.
pub(crate) fn parse_listing(xml: &str, listed: &str) -> Result<Vec<Child>, quick_xml::Error> {
    #[derive(PartialEq, Clone, Copy)]
    enum Reading {
        Nothing,
        Href,
        Length,
        Etag,
        Modified,
    }
    #[derive(Default)]
    struct Entry {
        href: String,
        size: String,
        etag: String,
        modified: String,
        is_dir: bool,
    }
    let mut reader = Reader::from_str(xml);
    let mut children = Vec::new();
    let mut entry = Entry::default();
    let mut reading = Reading::Nothing;
    let mut text = String::new();
    loop {
        match reader.read_event()? {
            Event::Start(tag) | Event::Empty(tag) => {
                let started = match tag.local_name().as_ref() {
                    "response" => {
                        entry = Entry::default();
                        None
                    }
                    "href" => Some(Reading::Href),
                    "getcontentlength" => Some(Reading::Length),
                    "getetag" => Some(Reading::Etag),
                    "getlastmodified" => Some(Reading::Modified),
                    "collection" => {
                        entry.is_dir = true;
                        None
                    }
                    _ => None,
                };
                if let Some(started) = started {
                    (reading, text) = (started, String::new());
                }
            }
            Event::Text(t) if reading != Reading::Nothing => text.push_str(&t.xml10_content()),
            Event::CData(t) if reading != Reading::Nothing => text.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if reading != Reading::Nothing => {
                match (r.resolve_char_ref()?, &*r) {
                    (Some(c), _) => text.push(c),
                    (None, "amp") => text.push('&'),
                    (None, "lt") => text.push('<'),
                    (None, "gt") => text.push('>'),
                    (None, "quot") => text.push('"'),
                    (None, "apos") => text.push('\''),
                    _ => {}
                }
            }
            Event::End(tag) => match tag.local_name().as_ref() {
                "href" | "getcontentlength" | "getetag" | "getlastmodified" => {
                    // A missing value (a 404 `propstat`) stays as it was.
                    let value = text.trim().to_owned();
                    match reading {
                        Reading::Href => entry.href = value,
                        Reading::Length => entry.size = value,
                        Reading::Etag => entry.etag = value,
                        Reading::Modified => entry.modified = value,
                        Reading::Nothing => {}
                    }
                    reading = Reading::Nothing;
                }
                "response" => {
                    let path = url_path(&entry.href);
                    let name = path.rsplit('/').next().unwrap_or("");
                    if path != listed && !name.is_empty() && !name.starts_with('.') {
                        let kind = if entry.is_dir {
                            ChildKind::Dir
                        } else if entry.etag.is_empty() {
                            ChildKind::File {
                                marker: format!("{}-{}", entry.modified, entry.size),
                            }
                        } else {
                            ChildKind::File {
                                marker: entry.etag.clone(),
                            }
                        };
                        children.push(Child {
                            name: name.to_owned(),
                            kind,
                        });
                    }
                }
                _ => {}
            },
            Event::Eof => return Ok(children),
            _ => {}
        }
    }
}

/// The `Authorization` header value for Basic auth (RFC 7617).
pub(crate) fn basic_auth(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::encode(format!("{username}:{password}").as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTH: &str = "Basic dTpw";

    fn dav() -> WebDav {
        WebDav::new(" https://host/dav/ ", "u", "p", "Drawings").unwrap()
    }

    fn answer(status: u16, body: &str) -> HttpAnswer {
        HttpAnswer {
            status,
            body: body.as_bytes().to_vec(),
        }
    }

    /// The method and URL of a request, and that it carries the login.
    fn line(request: &HttpRequest) -> String {
        assert_eq!(request.headers[0], ("Authorization", AUTH.to_owned()));
        format!("{} {}", request.method, request.url)
    }

    /// Runs `exchange` against the canned `answers`, returning the requests
    /// sent and the result.
    fn run<E: Exchange>(mut exchange: E, answers: &[HttpAnswer]) -> (Vec<String>, E::Output) {
        let mut sent = vec![line(&exchange.start())];
        for given in answers {
            match exchange.answer(given.clone()) {
                Turn::Send(request) => sent.push(line(&request)),
                Turn::Done(output) => return (sent, output),
            }
        }
        panic!("still asking after {sent:?}");
    }

    #[test]
    fn only_https_urls_are_taken() {
        for url in ["http://x", "x", "ftp://x", "", "https:/x"] {
            assert_eq!(
                WebDav::new(url, "u", "p", "Drawings").unwrap_err(),
                ConnectError::NotHttps,
                "{url:?}"
            );
        }
        assert!(WebDav::new("HTTPS://x", "u", "p", "Drawings").is_ok());
    }

    #[test]
    fn connecting_checks_the_server_then_finds_the_folder() {
        let answers = [answer(207, ""), answer(207, ""), answer(207, "")];
        let (sent, result) = run(dav().connect(), &answers);
        assert_eq!(
            sent,
            [
                "PROPFIND https://host/dav",
                "PROPFIND https://host/dav/pkm",
                "PROPFIND https://host/dav/pkm/Drawings"
            ]
        );
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn connecting_creates_a_missing_folder() {
        let answers = [
            answer(207, ""),
            answer(404, ""),
            answer(201, ""),
            answer(404, ""),
            answer(201, ""),
        ];
        let (sent, result) = run(dav().connect(), &answers);
        assert_eq!(sent[2], "MKCOL https://host/dav/pkm");
        assert_eq!(sent[3], "PROPFIND https://host/dav/pkm/Drawings");
        assert_eq!(sent[4], "MKCOL https://host/dav/pkm/Drawings");
        assert_eq!(result, Ok(()));

        // `pkm` exists already (405), and so does the Context's folder.
        let answers = [
            answer(207, ""),
            answer(404, ""),
            answer(405, ""),
            answer(404, ""),
            answer(405, ""),
        ];
        assert_eq!(run(dav().connect(), &answers).1, Ok(()));
    }

    #[test]
    fn connecting_names_what_is_wrong() {
        let first = |status| run(dav().connect(), &[answer(status, "")]).1;
        assert_eq!(first(401), Err(ConnectError::WrongCredentials));
        assert_eq!(first(200), Err(ConnectError::NotWebDav));
        assert_eq!(first(404), Err(ConnectError::NotWebDav));
        assert_eq!(first(503), Err(ConnectError::Unreachable));

        let second = |status| run(dav().connect(), &[answer(207, ""), answer(status, "")]).1;
        assert_eq!(second(403), Err(ConnectError::NotWebDav));

        let made = run(
            dav().connect(),
            &[answer(207, ""), answer(404, ""), answer(403, "")],
        );
        assert_eq!(made.1, Err(ConnectError::NotWebDav));
    }

    #[test]
    fn connecting_without_an_answer_is_unreachable() {
        let mut connect = dav().connect();
        connect.start();
        let error = io::Error::other("offline");
        assert_eq!(connect.failed(error), Err(ConnectError::Unreachable));
    }

    fn dir_xml(entries: &[(&str, bool)]) -> String {
        let responses: String = entries
            .iter()
            .map(|(href, dir)| {
                let kind = if *dir {
                    "<D:collection/>"
                } else {
                    "<D:getetag>\"e\"</D:getetag>"
                };
                format!("<D:response><D:href>{href}</D:href><D:propstat><D:prop><D:resourcetype>{}</D:resourcetype>{kind}</D:prop></D:propstat></D:response>", if *dir { "<D:collection/>" } else { "" })
            })
            .collect();
        format!(r#"<D:multistatus xmlns:D="DAV:">{responses}</D:multistatus>"#)
    }

    #[test]
    fn scanning_asks_for_depth_one_of_each_directory_and_reads_the_answers() {
        let mut scan = dav().scan();
        let request = scan.start();
        assert_eq!(line(&request), "PROPFIND https://host/dav/pkm/Drawings");
        assert!(request.headers.contains(&("Depth", "1".to_owned())));
        let top = dir_xml(&[
            ("/dav/pkm/Drawings/", true),
            ("/dav/pkm/Drawings/a", false),
            ("/dav/pkm/Drawings/Work%20x", true),
            ("/dav/pkm/Drawings/Ideas", true),
        ]);
        let Turn::Send(next) = scan.answer(answer(207, &top)) else {
            panic!("no second request");
        };
        let mut visited = vec![line(&next)];
        // Work x holds a file and a directory; Ideas holds nothing.
        let inner = dir_xml(&[
            ("/dav/pkm/Drawings/Ideas/", true),
            ("/dav/pkm/Drawings/Ideas/.hidden", false),
        ]);
        let work = dir_xml(&[
            ("https://host/dav/pkm/Drawings/Work%20x/", true),
            ("https://host/dav/pkm/Drawings/Work%20x/b.md", false),
            ("https://host/dav/pkm/Drawings/Work%20x/Sub", true),
        ]);
        let sub = dir_xml(&[("/dav/pkm/Drawings/Work%20x/Sub/", true)]);
        let mut turn = scan.answer(answer(207, &inner));
        for body in [&work, &sub] {
            let Turn::Send(next) = turn else {
                panic!("not asked");
            };
            visited.push(line(&next));
            turn = scan.answer(answer(207, body));
        }
        let Turn::Done(Ok(mut tree)) = turn else {
            panic!("not done");
        };
        visited.sort();
        assert_eq!(
            visited,
            [
                "PROPFIND https://host/dav/pkm/Drawings/Ideas",
                "PROPFIND https://host/dav/pkm/Drawings/Work%20x",
                "PROPFIND https://host/dav/pkm/Drawings/Work%20x/Sub",
            ]
        );
        tree.blobs.sort_by(|a, b| a.name.cmp(&b.name));
        let names: Vec<_> = tree.blobs.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Work x/b.md", "a"]);
        tree.empty_dirs.sort();
        assert_eq!(tree.empty_dirs, ["Ideas", "Work x/Sub"]);
    }

    #[test]
    fn scanning_a_missing_folder_is_empty_and_problems_are_errors() {
        assert_eq!(
            run(dav().scan(), &[answer(404, "")]).1.unwrap(),
            Tree::default()
        );
        let kind = |status| {
            run(dav().scan(), &[answer(status, "")])
                .1
                .unwrap_err()
                .kind()
        };
        assert_eq!(kind(401), io::ErrorKind::PermissionDenied);
        assert_eq!(kind(507), io::ErrorKind::StorageFull);
        assert_eq!(kind(500), io::ErrorKind::Other);
    }

    #[test]
    fn creating_a_directory_makes_each_folder_from_pkm_down() {
        let answers = [
            answer(405, ""),
            answer(405, ""),
            answer(201, ""),
            answer(201, ""),
        ];
        let (sent, result) = run(dav().create_dir("Work/Plans"), &answers);
        assert_eq!(
            sent,
            [
                "MKCOL https://host/dav/pkm",
                "MKCOL https://host/dav/pkm/Drawings",
                "MKCOL https://host/dav/pkm/Drawings/Work",
                "MKCOL https://host/dav/pkm/Drawings/Work/Plans",
            ]
        );
        assert!(result.is_ok());
        let refused = [answer(201, ""), answer(201, ""), answer(409, "")];
        assert!(run(dav().create_dir("a/b"), &refused).1.is_err());
    }

    #[test]
    fn getting_gives_the_body_none_or_an_error() {
        let (sent, result) = run(dav().get("a b"), &[answer(200, "hi")]);
        assert_eq!(sent, ["GET https://host/dav/pkm/Drawings/a%20b"]);
        let (sent, _) = run(dav().get("Work/a b"), &[answer(200, "hi")]);
        assert_eq!(sent, ["GET https://host/dav/pkm/Drawings/Work/a%20b"]);
        assert_eq!(result.unwrap(), Some(b"hi".to_vec()));
        assert_eq!(run(dav().get("x"), &[answer(404, "")]).1.unwrap(), None);
        let error = run(dav().get("x"), &[answer(401, "")]).1.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(run(dav().get("x"), &[answer(500, "")]).1.is_err());
    }

    #[test]
    fn deleting_a_missing_blob_is_fine() {
        let (sent, result) = run(dav().delete("x"), &[answer(404, "")]);
        assert_eq!(sent, ["DELETE https://host/dav/pkm/Drawings/x"]);
        assert!(result.is_ok());
        assert!(run(dav().delete("x"), &[answer(204, "")]).1.is_ok());
        assert!(run(dav().delete("x"), &[answer(500, "")]).1.is_err());
    }

    #[test]
    fn putting_uploads_to_incoming_then_moves() {
        let mut put = dav().put("a", "t1", b"data".to_vec(), true);
        let upload = put.start();
        assert_eq!(
            line(&upload),
            "PUT https://host/dav/pkm/Drawings/.incoming/t1"
        );
        assert_eq!(upload.body, b"data");
        let Turn::Send(moved) = put.answer(answer(201, "")) else {
            panic!("no move");
        };
        assert_eq!(
            line(&moved),
            "MOVE https://host/dav/pkm/Drawings/.incoming/t1"
        );
        assert!(
            moved
                .headers
                .contains(&("Destination", "https://host/dav/pkm/Drawings/a".to_owned()))
        );
        assert!(moved.headers.contains(&("Overwrite", "T".to_owned())));
        assert!(matches!(put.answer(answer(204, "")), Turn::Done(Ok(()))));
    }

    #[test]
    fn putting_into_a_missing_directory_makes_it_and_moves_again() {
        let answers = [
            answer(201, ""), // PUT
            answer(409, ""), // MOVE: no directory
            answer(201, ""), // MKCOL Work
            answer(405, ""), // MKCOL Work/Plans: there already
            answer(201, ""), // MOVE
        ];
        let (sent, result) = run(dav().put("Work/Plans/a", "t", Vec::new(), true), &answers);
        let moved = "MOVE https://host/dav/pkm/Drawings/.incoming/t";
        assert_eq!(
            sent[1..],
            [
                moved,
                "MKCOL https://host/dav/pkm/Drawings/Work",
                "MKCOL https://host/dav/pkm/Drawings/Work/Plans",
                moved,
            ]
        );
        assert!(result.is_ok());
    }

    #[test]
    fn putting_gives_up_when_the_directories_do_not_help() {
        let answers = [
            answer(201, ""),
            answer(409, ""),
            answer(201, ""),
            answer(409, ""),
            answer(204, ""), // cleaning up
        ];
        let (sent, result) = run(dav().put("Work/a", "t", Vec::new(), true), &answers);
        assert_eq!(sent.len(), 5);
        assert!(sent[4].starts_with("DELETE "));
        assert!(result.is_err());
        // A flat name has no directories to make.
        let answers = [answer(201, ""), answer(409, ""), answer(204, "")];
        assert!(
            run(dav().put("a", "t", Vec::new(), true), &answers)
                .1
                .is_err()
        );
    }

    #[test]
    fn putting_creates_the_incoming_folder_when_it_may_be_missing() {
        let answers = [answer(201, ""), answer(201, ""), answer(201, "")];
        let (sent, result) = run(dav().put("a", "t", Vec::new(), false), &answers);
        let methods: Vec<_> = sent.iter().map(|s| s.split(' ').next().unwrap()).collect();
        assert_eq!(methods, ["MKCOL", "PUT", "MOVE"]);
        assert_eq!(sent[0], "MKCOL https://host/dav/pkm/Drawings/.incoming");
        assert!(result.is_ok());
        // It exists already.
        let answers = [answer(405, ""), answer(201, ""), answer(201, "")];
        assert!(
            run(dav().put("a", "t", Vec::new(), false), &answers)
                .1
                .is_ok()
        );
    }

    #[test]
    fn putting_creates_missing_parent_folders_of_a_context_added_after_connecting() {
        // 409: the parent is missing. `pkm/Drawings` is; then `pkm` too.
        let mkcols = [
            "MKCOL https://host/dav/pkm/Drawings/.incoming",
            "MKCOL https://host/dav/pkm/Drawings",
            "MKCOL https://host/dav/pkm",
        ];
        for missing in 1..=2 {
            // 409s down to the missing folder; then it, each folder above
            // and the PUT and MOVE succeed.
            let mut answers = vec![answer(409, ""); missing];
            answers.extend(vec![answer(201, ""); missing + 3]);
            let (sent, result) = run(dav().put("a", "t", Vec::new(), false), &answers);
            let mut expected = mkcols[..=missing].to_vec();
            expected.extend(mkcols[..missing].iter().rev());
            assert_eq!(sent[..expected.len()], expected, "{missing}");
            assert!(sent[expected.len()].starts_with("PUT "), "{missing}");
            assert!(sent[expected.len() + 1].starts_with("MOVE "), "{missing}");
            assert!(result.is_ok(), "{missing}");
        }
    }

    #[test]
    fn putting_gives_up_when_a_folder_stays_missing() {
        // Even the Server URL is missing.
        let answers = [answer(409, ""), answer(409, ""), answer(409, "")];
        let (sent, result) = run(dav().put("a", "t", Vec::new(), false), &answers);
        assert_eq!(sent.len(), 3);
        assert!(result.is_err());
        // A made parent doesn't help.
        let answers = [answer(409, ""), answer(201, ""), answer(409, "")];
        let (sent, result) = run(dav().put("a", "t", Vec::new(), false), &answers);
        assert_eq!(sent.len(), 3);
        assert!(result.is_err());
    }

    #[test]
    fn a_failed_upload_deletes_the_temporary_blob_and_reports_why() {
        let put = |answers: &[HttpAnswer]| run(dav().put("a", "t", Vec::new(), true), answers);
        for (answers, kind, last) in [
            (
                vec![answer(507, ""), answer(204, "")],
                io::ErrorKind::StorageFull,
                1,
            ),
            (
                vec![answer(500, ""), answer(204, "")],
                io::ErrorKind::Other,
                1,
            ),
            (
                vec![answer(201, ""), answer(401, ""), answer(204, "")],
                io::ErrorKind::PermissionDenied,
                2,
            ),
            (
                vec![answer(201, ""), answer(409, ""), answer(204, "")],
                io::ErrorKind::Other,
                2,
            ),
        ] {
            let (sent, result) = put(&answers);
            assert_eq!(
                sent[last],
                "DELETE https://host/dav/pkm/Drawings/.incoming/t"
            );
            assert_eq!(result.unwrap_err().kind(), kind);
        }
        let error = put(&[answer(500, ""), answer(500, "")]).1.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
    }

    #[test]
    fn a_failed_cleanup_keeps_the_first_error() {
        let mut put = dav().put("a", "t", Vec::new(), true);
        put.start();
        let Turn::Send(_) = put.answer(answer(507, "")) else {
            panic!("no cleanup");
        };
        let error = put.failed(io::Error::other("offline"));
        assert_eq!(error.unwrap_err().kind(), io::ErrorKind::StorageFull);
    }

    #[test]
    fn rejected_credentials_and_a_full_server_differ_from_being_offline() {
        let rejected = refuse(401).unwrap_err();
        assert_eq!(rejected.kind(), io::ErrorKind::PermissionDenied);
        let full = refuse(507).unwrap_err();
        assert_eq!(full.kind(), io::ErrorKind::StorageFull);
        for status in [200, 207, 404, 409, 500, 503] {
            assert!(refuse(status).is_ok(), "{status}");
        }
    }

    #[test]
    fn basic_auth_is_base64_of_user_and_password() {
        // RFC 7617's example, and each padding length.
        assert_eq!(
            basic_auth("Aladdin", "open sesame"),
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
        assert_eq!(basic_auth("a", "b"), "Basic YTpi");
        assert_eq!(basic_auth("ab", "c"), "Basic YWI6Yw==");
        assert_eq!(basic_auth("a", ""), "Basic YTo=");
    }

    #[test]
    fn parsing_gives_the_children_but_no_hidden_files_or_the_listed_directory() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
            <D:multistatus xmlns:D="DAV:">
              <D:response><D:href>/dav/pkm/</D:href>
                <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat>
              </D:response>
              <D:response><D:href>/dav/pkm/a%20b&amp;c.excalidraw</D:href>
                <D:propstat><D:prop><D:resourcetype/><D:getcontentlength>12</D:getcontentlength><D:getetag>"e1"</D:getetag></D:prop></D:propstat>
              </D:response>
              <response xmlns="DAV:"><href>https://host/dav/pkm/sub/</href>
                <propstat><prop><resourcetype><collection></collection></resourcetype></prop></propstat>
              </response>
              <D:response><D:href>/dav/pkm/.incoming-1</D:href>
                <D:propstat><D:prop><D:getcontentlength>3</D:getcontentlength></D:prop></D:propstat>
              </D:response>
              <D:response><D:href>/dav/pkm/x</D:href>
                <D:propstat><D:prop><D:getcontentlength>7</D:getcontentlength><D:getlastmodified>Mon, 01 Jan 2024 10:00:00 GMT</D:getlastmodified></D:prop></D:propstat>
                <D:propstat><D:prop><D:foo/></D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat>
              </D:response>
            </D:multistatus>"#;

        let children = parse_listing(xml, "/dav/pkm").unwrap();

        // The ETag is the marker; without one, modified time and size are.
        let expected = [
            ("a b&c.excalidraw", "\"e1\""),
            ("x", "Mon, 01 Jan 2024 10:00:00 GMT-7"),
        ];
        let listed: Vec<_> = children
            .iter()
            .filter_map(|c| match &c.kind {
                ChildKind::File { marker } => Some((c.name.as_str(), marker.as_str())),
                ChildKind::Dir => None,
            })
            .collect();
        assert_eq!(listed, expected);
        let dirs: Vec<_> = children
            .iter()
            .filter(|c| matches!(c.kind, ChildKind::Dir))
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(dirs, ["sub"]);
    }
}
