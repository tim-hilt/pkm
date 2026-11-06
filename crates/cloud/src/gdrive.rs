//! Google Drive as a Cloud Provider (ADR-0012): the Drawings are Synced with
//! `My Drive/ExcalidrawNative`.
//!
//! Signs in with OAuth and PKCE and asks only for `drive.file`, so the app
//! sees nothing but the files it created itself. Blocking, no async runtime.
//! Deletes are permanent, not moved to the trash.

use std::collections::HashSet;
use std::fmt;
use std::io::{self, Read};
use std::sync::{Arc, Mutex};

use excalidraw_native_core::json::{self, Object, Value};
use ureq::Body;
use ureq::http::Response;

use crate::http::api_agent;
use crate::oauth::{OAuthConfig, OnRotate, Session, SignIn as OAuthSignIn};
use crate::percent::encode;
use crate::storage::{Blob, ConnectError, Storage, check_name, unique_name};

/// The folder in My Drive that holds everything the app stores.
const FOLDER: &str = "ExcalidrawNative";
const FOLDER_TYPE: &str = "application/vnd.google-apps.folder";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

/// Where Google's endpoints are; the tests point them at a fake server.
#[derive(Debug, Clone)]
pub(crate) struct Hosts {
    auth: String,
    token: String,
    /// `…/drive/v3`.
    api: String,
    /// `…/upload/drive/v3`.
    upload: String,
}

impl Hosts {
    fn google() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            api: "https://www.googleapis.com/drive/v3".into(),
            upload: "https://www.googleapis.com/upload/drive/v3".into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(base: &str) -> Self {
        Self {
            auth: format!("{base}/auth"),
            token: format!("{base}/token"),
            api: format!("{base}/drive/v3"),
            upload: format!("{base}/upload/drive/v3"),
        }
    }

    fn https_only(&self) -> bool {
        self.token.starts_with("https://")
    }

    /// The scheme Google redirects to after sign-in is the client ID reversed
    /// (`123-abc.apps.googleusercontent.com` → `com.googleusercontent.apps.123-abc`).
    fn config(&self, client_id: &str) -> OAuthConfig {
        let id = client_id
            .strip_suffix(".apps.googleusercontent.com")
            .unwrap_or(client_id);
        OAuthConfig {
            client_id: client_id.to_owned(),
            auth_url: self.auth.clone(),
            token_url: self.token.clone(),
            scope: SCOPE.to_owned(),
            redirect_uri: format!("com.googleusercontent.apps.{id}:/oauth2redirect"),
            // A refresh token is always handed out, also on a second sign-in.
            extra: vec![
                ("access_type".into(), "offline".into()),
                ("prompt".into(), "consent".into()),
            ],
        }
    }
}

/// The account signed in to, and what the client keeps to resume it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// To keep in the keychain; `GoogleDriveStorage::resume` takes it.
    pub refresh_token: String,
    /// The account's email address, for the user to see.
    pub email: String,
}

/// A sign-in under way. The client shows `url()` in the system's web
/// authentication session, listening for `callback_scheme()`, and hands the
/// redirect it ends with to `finish`. A cancelled page is nothing to finish.
pub struct SignIn {
    inner: OAuthSignIn,
    hosts: Hosts,
}

impl SignIn {
    /// Starts signing in with the OAuth client `client_id`.
    pub fn start(client_id: &str) -> Result<Self, ConnectError> {
        Self::start_with(Hosts::google(), client_id)
    }

    pub(crate) fn start_with(hosts: Hosts, client_id: &str) -> Result<Self, ConnectError> {
        let inner =
            OAuthSignIn::start(hosts.config(client_id)).map_err(|_| ConnectError::SignInFailed)?;
        Ok(Self { inner, hosts })
    }

    /// The sign-in page.
    pub fn url(&self) -> &str {
        self.inner.url()
    }

    /// The URL scheme of the redirect.
    pub fn callback_scheme(&self) -> String {
        self.inner.callback_scheme()
    }

    /// Finishes with the `redirect` URL the page ended at: exchanges the code,
    /// makes sure `My Drive/ExcalidrawNative` exists and tells the account.
    /// Refusing consent is `AccessDenied`. Blocks while it talks to Google.
    pub fn finish(self, redirect: &str) -> Result<Account, ConnectError> {
        let agent = api_agent(self.hosts.https_only());
        let session = self
            .inner
            .finish(agent.clone(), redirect, Arc::new(|_| {}))?;
        let storage = GoogleDriveStorage::from_session(self.hosts, session);
        storage.folder_id().map_err(|e| match e.kind() {
            io::ErrorKind::PermissionDenied => ConnectError::AccessDenied,
            _ => ConnectError::Unreachable,
        })?;
        let email = storage.email().unwrap_or_else(|_| "Google account".into());
        Ok(Account {
            refresh_token: storage.session.refresh_token(),
            email,
        })
    }
}

/// `My Drive/ExcalidrawNative`. Tokens revoked are `PermissionDenied`, a full
/// Drive is `StorageFull`, anything else that fails is another kind.
pub struct GoogleDriveStorage {
    session: Session,
    hosts: Hosts,
    /// The ID of the `ExcalidrawNative` folder, once known.
    folder: Mutex<Option<String>>,
}

impl fmt::Debug for GoogleDriveStorage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // No tokens.
        f.debug_struct("GoogleDriveStorage").finish_non_exhaustive()
    }
}

impl GoogleDriveStorage {
    /// The storage of an account signed in before, from its refresh token.
    /// Contacts nobody, so it works offline; errors show on use. `on_rotate`
    /// hears of a refresh token that replaces `refresh_token`, to be kept.
    pub fn resume(
        client_id: &str,
        refresh_token: &str,
        on_rotate: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        Self::resume_with(
            Hosts::google(),
            client_id,
            refresh_token,
            Arc::new(on_rotate),
        )
    }

    pub(crate) fn resume_with(
        hosts: Hosts,
        client_id: &str,
        refresh_token: &str,
        on_rotate: OnRotate,
    ) -> Self {
        let agent = api_agent(hosts.https_only());
        let session = Session::resume(
            agent.clone(),
            hosts.config(client_id),
            refresh_token.to_owned(),
            on_rotate,
        );
        Self::from_session(hosts, session)
    }

    fn from_session(hosts: Hosts, session: Session) -> Self {
        Self {
            session,
            hosts,
            folder: Mutex::new(None),
        }
    }

    /// Sends a request. Any answer is returned, whatever its status, except
    /// for refused access (see `Session::send`, or 403 with a reason that is
    /// not about this request) and a full Drive.
    fn send(
        &self,
        method: &str,
        url: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> io::Result<Response<Body>> {
        let headers: Vec<_> = content_type
            .map(|t| ("Content-Type", t))
            .into_iter()
            .collect();
        let mut response = self.session.send(method, url, &headers, body)?;
        if response.status().as_u16() == 403 {
            let text = response
                .body_mut()
                .read_to_string()
                .map_err(ureq::Error::into_io)?;
            return Err(forbidden(&text));
        }
        Ok(response)
    }

    /// The JSON answer to a request, which must succeed.
    fn json(&self, method: &str, url: &str, body: Option<&Value>) -> io::Result<Value> {
        let body = body.map(Value::to_json).unwrap_or_default();
        let content_type = (!body.is_empty()).then_some("application/json");
        let mut response = self.send(method, url, content_type, body.as_bytes())?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(unexpected(method, status));
        }
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(ureq::Error::into_io)?;
        json::parse(&text).map_err(io::Error::other)
    }

    /// Files matching the Drive query `q`, oldest first: their `id`, `name`
    /// and `version`. Pages are followed.
    fn query(&self, q: &str, limit: Option<u32>) -> io::Result<Vec<(String, String, String)>> {
        let mut files = Vec::new();
        let mut page = String::new();
        loop {
            let url = format!(
                "{}/files?q={}&fields={}&orderBy=createdTime&pageSize={}{page}",
                self.hosts.api,
                encode(q),
                encode("nextPageToken,files(id,name,version)"),
                limit.unwrap_or(1000),
            );
            let answer = self.json("GET", &url, None)?;
            let text = |file: &Value, key: &str| {
                file.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            for file in answer.get("files").and_then(Value::as_array).unwrap_or(&[]) {
                files.push((text(file, "id"), text(file, "name"), text(file, "version")));
            }
            match answer.get("nextPageToken").and_then(Value::as_str) {
                Some(token) if limit.is_none() => page = format!("&pageToken={}", encode(token)),
                _ => return Ok(files),
            }
        }
    }

    /// The ID of the `ExcalidrawNative` folder: found, or created now.
    fn folder_id(&self) -> io::Result<String> {
        let mut known = self.folder.lock().unwrap();
        if let Some(id) = &*known {
            return Ok(id.clone());
        }
        let q = format!(
            "mimeType = '{FOLDER_TYPE}' and name = '{FOLDER}' and 'root' in parents and trashed = false"
        );
        let id = match self.query(&q, Some(1))?.into_iter().next() {
            Some((id, ..)) => id,
            None => {
                let mut meta = Object::new();
                meta.insert("name".into(), Value::String(FOLDER.into()));
                meta.insert("mimeType".into(), Value::String(FOLDER_TYPE.into()));
                let created = self.json(
                    "POST",
                    &format!("{}/files?fields=id", self.hosts.api),
                    Some(&Value::Object(meta)),
                )?;
                created
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| io::Error::other("no folder id"))?
                    .to_owned()
            }
        };
        *known = Some(id.clone());
        Ok(id)
    }

    /// The ID of the file `name` in the folder; the oldest if there are two.
    fn file_id(&self, name: &str) -> io::Result<Option<String>> {
        let q = format!(
            "name = '{}' and '{}' in parents and trashed = false",
            escape(name),
            self.folder_id()?
        );
        Ok(self
            .query(&q, Some(1))?
            .into_iter()
            .next()
            .map(|(id, ..)| id))
    }

    fn email(&self) -> io::Result<String> {
        let about = self.json(
            "GET",
            &format!(
                "{}/about?fields={}",
                self.hosts.api,
                encode("user(emailAddress)")
            ),
            None,
        )?;
        about
            .get("user")
            .and_then(|user| user.get("emailAddress"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| io::Error::other("no email address"))
    }

    fn create(&self, name: &str, content: &[u8]) -> io::Result<u16> {
        let mut meta = Object::new();
        meta.insert("name".into(), Value::String(name.into()));
        meta.insert("mimeType".into(), Value::String(DRAWING_TYPE.into()));
        meta.insert(
            "parents".into(),
            Value::Array(vec![Value::String(self.folder_id()?)]),
        );
        let boundary = format!("excalidraw-native-{}", unique_name());
        let mut body = format!(
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{}\r\n--{boundary}\r\nContent-Type: {DRAWING_TYPE}\r\n\r\n",
            Value::Object(meta).to_json()
        )
        .into_bytes();
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{boundary}--").as_bytes());
        let url = format!("{}/files?uploadType=multipart&fields=id", self.hosts.upload);
        let content_type = format!("multipart/related; boundary={boundary}");
        let response = self.send("POST", &url, Some(&content_type), &body)?;
        Ok(response.status().as_u16())
    }
}

const DRAWING_TYPE: &str = "application/json";

/// `text` as the inside of a quoted Drive query string.
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\'', "\\'")
}

fn unexpected(method: &str, status: u16) -> io::Error {
    io::Error::other(format!("{method} answered with status {status}"))
}

/// The error of a 403 answer: a full Drive is `StorageFull`, a lost
/// permission is `PermissionDenied`, anything else (rate limits) is not told
/// from being offline, so Sync retries.
fn forbidden(body: &str) -> io::Error {
    let reason = json::parse(body)
        .ok()
        .and_then(|v| {
            v.get("error")?
                .get("errors")?
                .as_array()?
                .first()?
                .get("reason")?
                .as_str()
                .map(str::to_owned)
        })
        .unwrap_or_default();
    match reason.as_str() {
        "storageQuotaExceeded" => {
            io::Error::new(io::ErrorKind::StorageFull, "Google Drive is full")
        }
        "insufficientPermissions" | "insufficientFilePermissions" | "authError" => {
            io::Error::new(io::ErrorKind::PermissionDenied, "403 Forbidden")
        }
        _ => io::Error::other(format!("403 Forbidden: {reason}")),
    }
}

impl Storage for GoogleDriveStorage {
    fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()> {
        check_name(name)?;
        // A Drawing is uploaded whole, in one request that Drive applies
        // entirely or not at all; read first, so a failing `data` sends nothing.
        let mut content = Vec::new();
        data.read_to_end(&mut content)?;
        let status = match self.file_id(name)? {
            Some(id) => {
                let url = format!("{}/files/{id}?uploadType=media", self.hosts.upload);
                self.send("PATCH", &url, Some(DRAWING_TYPE), &content)?
                    .status()
                    .as_u16()
            }
            None => self.create(name, &content)?,
        };
        if (200..300).contains(&status) {
            return Ok(());
        }
        if status == 404 {
            // The folder was deleted from outside: look for it anew.
            *self.folder.lock().unwrap() = None;
        }
        Err(unexpected("PUT", status))
    }

    fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
        check_name(name)?;
        let Some(id) = self.file_id(name)? else {
            return Ok(None);
        };
        let url = format!("{}/files/{id}?alt=media", self.hosts.api);
        let response = self.send("GET", &url, None, &[])?;
        match response.status().as_u16() {
            200..=299 => Ok(Some(Box::new(response.into_body().into_reader()))),
            404 => Ok(None),
            status => Err(unexpected("GET", status)),
        }
    }

    fn list(&self) -> io::Result<Vec<Blob>> {
        let q = format!("'{}' in parents and trashed = false", self.folder_id()?);
        let mut seen = HashSet::new();
        let mut blobs = Vec::new();
        for (_, name, version) in self.query(&q, None)? {
            // Two files of one name: the oldest counts, as for `get`.
            if check_name(&name).is_ok() && seen.insert(name.clone()) {
                blobs.push(Blob {
                    name,
                    marker: version,
                });
            }
        }
        Ok(blobs)
    }

    fn delete(&self, name: &str) -> io::Result<()> {
        check_name(name)?;
        let Some(id) = self.file_id(name)? else {
            return Ok(());
        };
        // Permanent: Drive's DELETE skips the trash.
        let response = self.send(
            "DELETE",
            &format!("{}/files/{id}", self.hosts.api),
            None,
            &[],
        )?;
        match response.status().as_u16() {
            200..=299 | 404 => Ok(()),
            status => Err(unexpected("DELETE", status)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_google::{CLIENT_ID, EMAIL, FakeGoogle};
    use crate::storage::contract::{names, put_bytes, storage_contract_tests};
    use crate::storage::get_bytes;

    /// Signs in at `fake` as the user does: consenting on the page.
    fn sign_in(fake: &FakeGoogle) -> Account {
        let sign_in = SignIn::start_with(fake.hosts(), CLIENT_ID).unwrap();
        let redirect = fake.browser(sign_in.url(), true);
        sign_in.finish(&redirect).unwrap()
    }

    fn resumed(fake: &FakeGoogle, refresh_token: &str) -> GoogleDriveStorage {
        GoogleDriveStorage::resume_with(fake.hosts(), CLIENT_ID, refresh_token, Arc::new(|_| {}))
    }

    fn drive_storage() -> Option<(FakeGoogle, GoogleDriveStorage)> {
        let fake = FakeGoogle::start();
        let account = sign_in(&fake);
        let storage = resumed(&fake, &account.refresh_token);
        Some((fake, storage))
    }

    mod google_drive_storage {
        use super::*;
        storage_contract_tests!(drive_storage);
    }

    #[test]
    fn signing_in_gives_the_account_and_makes_the_folder_in_my_drive() {
        let fake = FakeGoogle::start();

        let account = sign_in(&fake);

        assert_eq!(account.email, EMAIL);
        assert!(!account.refresh_token.is_empty());
        let files = fake.files();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "ExcalidrawNative");
        assert_eq!(files[0].parent, "root");
        assert_eq!(files[0].mime, FOLDER_TYPE);
    }

    #[test]
    fn the_sign_in_page_asks_for_the_file_scope_only_and_listens_for_the_reversed_client_id() {
        let sign_in = SignIn::start(CLIENT_ID).unwrap();

        assert!(
            sign_in
                .url()
                .starts_with("https://accounts.google.com/o/oauth2/v2/auth?")
        );
        let scope = format!(
            "scope={}",
            encode("https://www.googleapis.com/auth/drive.file")
        );
        assert!(sign_in.url().contains(&scope));
        assert_eq!(
            sign_in.callback_scheme(),
            "com.googleusercontent.apps.123-abc"
        );
    }

    #[test]
    fn refusing_consent_is_access_denied_and_touches_nothing() {
        let fake = FakeGoogle::start();
        let sign_in = SignIn::start_with(fake.hosts(), CLIENT_ID).unwrap();
        let redirect = fake.browser(sign_in.url(), false);

        assert_eq!(
            sign_in.finish(&redirect).unwrap_err(),
            ConnectError::AccessDenied
        );

        assert!(fake.requests().is_empty());
    }

    #[test]
    fn a_redirect_that_answers_another_sign_in_is_refused() {
        let fake = FakeGoogle::start();
        let other = SignIn::start_with(fake.hosts(), CLIENT_ID).unwrap();
        let redirect = fake.browser(other.url(), true);
        let sign_in = SignIn::start_with(fake.hosts(), CLIENT_ID).unwrap();

        assert_eq!(
            sign_in.finish(&redirect).unwrap_err(),
            ConnectError::SignInFailed
        );
        assert_eq!(
            SignIn::start_with(fake.hosts(), CLIENT_ID)
                .unwrap()
                .finish("https://evil.example/?code=x")
                .unwrap_err(),
            ConnectError::SignInFailed
        );
        assert!(fake.requests().is_empty());
    }

    #[test]
    fn an_unreachable_google_is_unreachable() {
        let fake = FakeGoogle::start();
        let hosts = fake.hosts();
        let sign_in = SignIn::start_with(hosts, CLIENT_ID).unwrap();
        let redirect = fake.browser(sign_in.url(), true);
        drop(fake);

        assert_eq!(
            sign_in.finish(&redirect).unwrap_err(),
            ConnectError::Unreachable
        );
    }

    #[test]
    fn resuming_uses_the_folder_that_is_there() {
        let (fake, storage) = drive_storage().unwrap();
        put_bytes(&storage, "Sketch.excalidraw", b"{}");
        let token = storage.session.refresh_token();

        let again = resumed(&fake, &token);

        assert_eq!(names(&again), ["Sketch.excalidraw"]);
        let folders = fake
            .files()
            .iter()
            .filter(|f| f.mime == FOLDER_TYPE)
            .count();
        assert_eq!(folders, 1);
    }

    #[test]
    fn resuming_contacts_nobody() {
        let fake = FakeGoogle::start();
        let _storage = resumed(&fake, "refresh-1");
        assert!(fake.requests().is_empty());
    }

    #[test]
    fn drawings_land_in_the_folder_and_other_files_are_left_alone() {
        let (fake, storage) = drive_storage().unwrap();
        fake.add_file("Sketch.excalidraw", "root", b"mine, in My Drive");

        put_bytes(&storage, "Sketch.excalidraw", b"{}");

        let files = fake.files();
        let folder = files.iter().find(|f| f.mime == FOLDER_TYPE).unwrap();
        let inside: Vec<_> = files.iter().filter(|f| f.parent == folder.id).collect();
        assert_eq!(inside.len(), 1);
        assert_eq!(inside[0].data, b"{}");
        let outside = files
            .iter()
            .find(|f| f.parent == "root" && f.mime != FOLDER_TYPE);
        assert_eq!(outside.unwrap().data, b"mine, in My Drive");
        assert_eq!(names(&storage), ["Sketch.excalidraw"]);
    }

    #[test]
    fn names_with_quotes_and_backslashes_work() {
        let (_fake, storage) = drive_storage().unwrap();
        for name in ["it's.excalidraw", r"back\slash.excalidraw"] {
            put_bytes(&storage, name, b"1");
            put_bytes(&storage, name, b"2");
            assert_eq!(get_bytes(&storage, name).unwrap().unwrap(), b"2");
        }
        assert_eq!(names(&storage).len(), 2);
    }

    #[test]
    fn listing_follows_all_pages() {
        let (fake, storage) = drive_storage().unwrap();
        let folder = fake.files()[0].id.clone();
        for i in 0..1005 {
            fake.add_file(&format!("D{i}.excalidraw"), &folder, b"{}");
        }

        assert_eq!(storage.list().unwrap().len(), 1005);
    }

    #[test]
    fn two_files_of_one_name_show_as_one_blob_the_oldest() {
        let (fake, storage) = drive_storage().unwrap();
        let folder = fake.files()[0].id.clone();
        fake.add_file("Sketch.excalidraw", &folder, b"old");
        fake.add_file("Sketch.excalidraw", &folder, b"new");

        assert_eq!(storage.list().unwrap().len(), 1);
        assert_eq!(
            get_bytes(&storage, "Sketch.excalidraw").unwrap().unwrap(),
            b"old"
        );
    }

    #[test]
    fn a_delete_is_permanent() {
        let (fake, storage) = drive_storage().unwrap();
        put_bytes(&storage, "Sketch.excalidraw", b"{}");

        storage.delete("Sketch.excalidraw").unwrap();

        // The file is gone, not moved to the trash: no update of any kind.
        assert!(fake.files().iter().all(|f| f.name != "Sketch.excalidraw"));
        let requests = fake.requests();
        assert!(
            requests
                .iter()
                .any(|r| r.starts_with("DELETE /drive/v3/files/"))
        );
        assert!(requests.iter().all(|r| !r.starts_with("PATCH /drive/v3/")));
    }

    #[test]
    fn an_expired_access_token_is_refreshed_by_the_crate() {
        let (fake, storage) = drive_storage().unwrap();
        put_bytes(&storage, "Sketch.excalidraw", b"{}");
        let before = fake.refreshes();

        fake.expire_access_tokens();

        assert_eq!(names(&storage), ["Sketch.excalidraw"]);
        assert_eq!(fake.refreshes(), before + 1);
        // The new token is kept: no refresh per call.
        names(&storage);
        assert_eq!(fake.refreshes(), before + 1);
    }

    #[test]
    fn a_rotated_refresh_token_is_handed_over_and_used_from_then_on() {
        let fake = FakeGoogle::start();
        let account = sign_in(&fake);
        fake.rotate_refresh_tokens();
        let handed = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&handed);
        let storage = GoogleDriveStorage::resume_with(
            fake.hosts(),
            CLIENT_ID,
            &account.refresh_token,
            Arc::new(move |token| log.lock().unwrap().push(token.to_owned())),
        );

        storage.list().unwrap();
        let rotated = handed.lock().unwrap().clone();
        assert_eq!(rotated.len(), 1);
        assert_ne!(rotated[0], account.refresh_token);

        // The old one is dead, the new one works: so does a later refresh.
        fake.expire_access_tokens();
        storage.list().unwrap();
        assert_eq!(handed.lock().unwrap().len(), 2);
        let from_keychain = resumed(&fake, &handed.lock().unwrap()[1]);
        assert!(from_keychain.list().is_ok());
    }

    #[test]
    fn revoked_access_is_permission_denied_not_offline() {
        let (fake, storage) = drive_storage().unwrap();
        put_bytes(&storage, "Sketch.excalidraw", b"{}");

        fake.revoke();

        let error = storage.list().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let error = storage.put("x", &mut &b"1"[..]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn a_full_drive_is_storage_full_but_reading_goes_on() {
        let (fake, storage) = drive_storage().unwrap();
        put_bytes(&storage, "Sketch.excalidraw", b"{}");

        fake.fill_up();

        let created = storage.put("New.excalidraw", &mut &b"1"[..]).unwrap_err();
        assert_eq!(created.kind(), io::ErrorKind::StorageFull);
        let updated = storage
            .put("Sketch.excalidraw", &mut &b"2"[..])
            .unwrap_err();
        assert_eq!(updated.kind(), io::ErrorKind::StorageFull);
        assert_eq!(
            get_bytes(&storage, "Sketch.excalidraw").unwrap().unwrap(),
            b"{}"
        );
        storage.delete("Sketch.excalidraw").unwrap();
    }

    #[test]
    fn no_answer_is_not_permission_denied() {
        let fake = FakeGoogle::start();
        let storage = resumed(&fake, "refresh-1");
        drop(fake);

        let error = storage.list().unwrap_err();

        assert_ne!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_ne!(error.kind(), io::ErrorKind::StorageFull);
    }

    #[test]
    fn the_folder_deleted_from_outside_is_made_again_on_the_next_try() {
        let (fake, storage) = drive_storage().unwrap();
        put_bytes(&storage, "Sketch.excalidraw", b"{}");
        fake.remove_all();

        assert!(storage.put("Sketch.excalidraw", &mut &b"2"[..]).is_err());
        put_bytes(&storage, "Sketch.excalidraw", b"3");

        assert_eq!(names(&storage), ["Sketch.excalidraw"]);
    }
}
