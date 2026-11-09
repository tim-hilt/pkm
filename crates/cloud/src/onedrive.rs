//! OneDrive as a Cloud Provider (ADR-0012): the Drawings are Synced with
//! `Apps/pkm`, the app folder of a personal OneDrive.
//!
//! Signs in with OAuth and PKCE and asks only for
//! `Files.ReadWrite.AppFolder` (and `offline_access`, for a refresh token).
//! Blobs are addressed by name below the app root, so there are no IDs to look
//! up, and Graph creates the folder on first use. Small Drawings upload in one
//! request, bigger ones through an upload session. Blocking, no async runtime.

use std::fmt;
use std::io::{self, Read};
use std::sync::Arc;

use pkm_drawing::json::{self, Value};
use ureq::Body;
use ureq::http::Response;

use crate::http::api_agent;
use crate::oauth::{OAuthConfig, OnRotate, Session, SignIn as OAuthSignIn};
use crate::percent::encode;
use crate::storage::{Blob, ConnectError, Storage, check_name};

const SCOPE: &str = "Files.ReadWrite.AppFolder offline_access";

/// Where the sign-in page redirects to: the scheme is the one of the app's
/// registration in Azure ("pkm", for personal Microsoft accounts).
pub(crate) const REDIRECT_URI: &str = "msauth.com.timhilt.pkm://auth";

/// Drawings up to this many bytes upload in one request, which Graph limits
/// to 4 MB.
const SIMPLE_LIMIT: usize = 4_000_000;
/// The size of a part of an upload session. Graph wants a multiple of 320 KiB.
const CHUNK_SIZE: usize = 10 << 20;

/// Where Microsoft's endpoints are; the tests point them at a fake server.
#[derive(Debug, Clone)]
pub(crate) struct Hosts {
    auth: String,
    token: String,
    /// `…/v1.0`.
    api: String,
}

impl Hosts {
    fn microsoft() -> Self {
        // `consumers`: personal accounts only.
        let login = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
        Self {
            auth: format!("{login}/authorize"),
            token: format!("{login}/token"),
            api: "https://graph.microsoft.com/v1.0".into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(base: &str) -> Self {
        Self {
            auth: format!("{base}/auth"),
            token: format!("{base}/token"),
            api: format!("{base}/v1.0"),
        }
    }

    fn https_only(&self) -> bool {
        self.token.starts_with("https://")
    }

    fn config(&self, client_id: &str) -> OAuthConfig {
        OAuthConfig {
            client_id: client_id.to_owned(),
            auth_url: self.auth.clone(),
            token_url: self.token.clone(),
            scope: SCOPE.to_owned(),
            redirect_uri: REDIRECT_URI.to_owned(),
            extra: Vec::new(),
        }
    }
}

/// The account signed in to, and what the client keeps to resume it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// To keep in the keychain; `OneDriveStorage::resume` takes it.
    pub refresh_token: String,
    /// The account's email address or name, for the user to see.
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
        Self::start_with(Hosts::microsoft(), client_id)
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

    /// Finishes with the `redirect` URL the page ended at: exchanges the code
    /// and tells the account. Refusing consent is `AccessDenied`, a work or
    /// school account is `UnsupportedAccount`. Blocks while it talks to
    /// Microsoft.
    pub fn finish(self, redirect: &str) -> Result<Account, ConnectError> {
        let agent = api_agent(self.hosts.https_only());
        let session = self.inner.finish(agent, redirect, Arc::new(|_| {}))?;
        let storage = OneDriveStorage::from_session(self.hosts, session);
        let email = storage.account().map_err(|e| match e {
            AccountError::Unsupported => ConnectError::UnsupportedAccount,
            AccountError::Io(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                ConnectError::AccessDenied
            }
            AccountError::Io(_) => ConnectError::Unreachable,
        })?;
        Ok(Account {
            refresh_token: storage.session.refresh_token(),
            email,
        })
    }
}

enum AccountError {
    Unsupported,
    Io(io::Error),
}

/// `Apps/pkm`. Tokens revoked are `PermissionDenied`, a full
/// OneDrive is `StorageFull`, anything else that fails is another kind.
pub struct OneDriveStorage {
    session: Session,
    hosts: Hosts,
    simple_limit: usize,
    chunk_size: usize,
}

impl fmt::Debug for OneDriveStorage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // No tokens.
        f.debug_struct("OneDriveStorage").finish_non_exhaustive()
    }
}

impl OneDriveStorage {
    /// The storage of an account signed in before, from its refresh token.
    /// Contacts nobody, so it works offline; errors show on use. `on_rotate`
    /// hears of a refresh token that replaces `refresh_token`, to be kept.
    pub fn resume(
        client_id: &str,
        refresh_token: &str,
        on_rotate: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        Self::resume_with(
            Hosts::microsoft(),
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
        let session = Session::resume(
            api_agent(hosts.https_only()),
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
            simple_limit: SIMPLE_LIMIT,
            chunk_size: CHUNK_SIZE,
        }
    }

    /// The URL of blob `name`, followed by `action` (`"/content"`, ...).
    fn blob_url(&self, name: &str, action: &str) -> String {
        format!("{}:/{}:{action}", self.root_url(), encode(name))
    }

    /// The app folder, which Graph creates on first access.
    fn root_url(&self) -> String {
        format!("{}/me/drive/special/approot", self.hosts.api)
    }

    /// Sends a request. Any answer is returned, whatever its status, except
    /// for refused access (see `Session::send`, or a 403) and a full OneDrive.
    fn send(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> io::Result<Response<Body>> {
        refuse(self.session.send(method, url, headers, body)?)
    }

    /// The JSON answer to a request, which must succeed.
    fn json(&self, method: &str, url: &str, body: Option<&Value>) -> io::Result<Value> {
        let body = body.map(Value::to_json).unwrap_or_default();
        let headers: &[_] = if body.is_empty() {
            &[]
        } else {
            &[("Content-Type", "application/json")]
        };
        read_json(self.send(method, url, headers, body.as_bytes())?, method)
    }

    /// Whose OneDrive it is: asking for the app folder needs no scope beyond
    /// `Files.ReadWrite.AppFolder`, creates it, and tells the account. Empty
    /// if OneDrive doesn't tell it without more permissions.
    fn account(&self) -> Result<String, AccountError> {
        let url = format!("{}?$select=createdBy,parentReference", self.root_url());
        let root = self.json("GET", &url, None).map_err(AccountError::Io)?;
        let drive_type = root
            .get("parentReference")
            .and_then(|p| p.get("driveType"))
            .and_then(Value::as_str);
        if drive_type.is_some_and(|kind| kind != "personal") {
            return Err(AccountError::Unsupported);
        }
        let user = root.get("createdBy").and_then(|c| c.get("user"));
        let text = |key| user?.get(key)?.as_str();
        Ok(text("email")
            .or_else(|| text("displayName"))
            .unwrap_or("Microsoft account")
            .to_owned())
    }

    /// Uploads `content` through an upload session, replacing blob `name`.
    /// Never resumed: an interrupted `put` starts over the next time.
    fn upload_in_parts(&self, name: &str, content: &[u8]) -> io::Result<()> {
        let mut item = json::Object::new();
        let replace = Value::String("replace".into());
        item.insert("@microsoft.graph.conflictBehavior".into(), replace);
        let mut body = json::Object::new();
        body.insert("item".into(), Value::Object(item));
        let session = self.json(
            "POST",
            &self.blob_url(name, "/createUploadSession"),
            Some(&Value::Object(body)),
        )?;
        let upload = session
            .get("uploadUrl")
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::other("the upload has no session"))?;

        let total = content.len();
        let mut start = 0;
        while start < total {
            let end = (start + self.chunk_size).min(total);
            // Graph wants the size of the whole with every part.
            let range = format!("bytes {start}-{}/{total}", end - 1);
            let response = self.session.send_to_handed_out_url(
                "PUT",
                upload,
                &[("Content-Range", &range)],
                &content[start..end],
            )?;
            match (response.status().as_u16(), end == total) {
                (202, false) | (200 | 201, true) => {}
                (status, _) => return Err(unexpected("upload", status)),
            }
            start = end;
        }
        Ok(())
    }
}

fn read_json(mut response: Response<Body>, what: &str) -> io::Result<Value> {
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(unexpected(what, status));
    }
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(ureq::Error::into_io)?;
    json::parse(&text).map_err(io::Error::other)
}

fn unexpected(what: &str, status: u16) -> io::Error {
    io::Error::other(format!("{what} answered with status {status}"))
}

/// The error a refusal stands for: a 507, or a 403 saying the quota is
/// reached, is `StorageFull`; any other 403 is a lost permission. Throttling
/// is not told from being offline, so Sync retries.
fn refuse(mut response: Response<Body>) -> io::Result<Response<Body>> {
    match response.status().as_u16() {
        507 => Err(io::Error::new(
            io::ErrorKind::StorageFull,
            "OneDrive is full",
        )),
        403 => {
            let text = response.body_mut().read_to_string().unwrap_or_default();
            if text.contains("quotaLimitReached") {
                Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "OneDrive is full",
                ))
            } else if text.contains("activityLimitReached") {
                Err(io::Error::other("403 Forbidden: throttled"))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "403 Forbidden",
                ))
            }
        }
        _ => Ok(response),
    }
}

impl Storage for OneDriveStorage {
    fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()> {
        check_name(name)?;
        // A Drawing is read first, so a failing `data` sends nothing, and is
        // replaced whole: Graph makes a blob visible only when all of it is there.
        let mut content = Vec::new();
        data.read_to_end(&mut content)?;
        if content.len() > self.simple_limit {
            return self.upload_in_parts(name, &content);
        }
        let headers = [("Content-Type", "application/octet-stream")];
        let response = self.send("PUT", &self.blob_url(name, "/content"), &headers, &content)?;
        match response.status().as_u16() {
            200..=299 => Ok(()),
            status => Err(unexpected("PUT", status)),
        }
    }

    fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
        check_name(name)?;
        let mut response = self.send("GET", &self.blob_url(name, "/content"), &[], &[])?;
        if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            // To a download URL that is authorized already, and refuses a token.
            let location = response
                .headers()
                .get("location")
                .and_then(|location| location.to_str().ok())
                .ok_or_else(|| io::Error::other("the download has no URL"))?;
            response = self
                .session
                .send_to_handed_out_url("GET", location, &[], &[])?;
        }
        match response.status().as_u16() {
            200..=299 => Ok(Some(Box::new(response.into_body().into_reader()))),
            404 => Ok(None),
            status => Err(unexpected("GET", status)),
        }
    }

    fn list(&self) -> io::Result<Vec<Blob>> {
        let mut next = Some(format!(
            "{}/children?$select=name,file,eTag&$top=200",
            self.root_url()
        ));
        let mut blobs = Vec::new();
        while let Some(url) = next {
            let page = read_json(self.send("GET", &url, &[], &[])?, "GET")?;
            for item in page.get("value").and_then(Value::as_array).unwrap_or(&[]) {
                let text = |key| item.get(key).and_then(Value::as_str);
                // Folders have no `file`.
                if let (Some(name), Some(marker), Some(_)) =
                    (text("name"), text("eTag"), item.get("file"))
                    && check_name(name).is_ok()
                {
                    blobs.push(Blob {
                        name: name.to_owned(),
                        marker: marker.to_owned(),
                    });
                }
            }
            next = page
                .get("@odata.nextLink")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        Ok(blobs)
    }

    fn delete(&self, name: &str) -> io::Result<()> {
        check_name(name)?;
        let response = self.send("DELETE", &self.blob_url(name, ""), &[], &[])?;
        match response.status().as_u16() {
            200..=299 | 404 => Ok(()),
            status => Err(unexpected("DELETE", status)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_onedrive::{CLIENT_ID, EMAIL, FakeMicrosoft};
    use crate::storage::contract::{names, put_bytes, storage_contract_tests};
    use crate::storage::get_bytes;
    use std::sync::Mutex;

    /// Signs in at `fake` as the user does: consenting on the page.
    fn sign_in(fake: &FakeMicrosoft) -> Account {
        let sign_in = SignIn::start_with(fake.hosts(), CLIENT_ID).unwrap();
        let redirect = fake.browser(sign_in.url(), true);
        sign_in.finish(&redirect).unwrap()
    }

    fn resumed(fake: &FakeMicrosoft, refresh_token: &str) -> OneDriveStorage {
        OneDriveStorage::resume_with(fake.hosts(), CLIENT_ID, refresh_token, Arc::new(|_| {}))
    }

    fn onedrive_storage() -> Option<(FakeMicrosoft, OneDriveStorage)> {
        let fake = FakeMicrosoft::start();
        let account = sign_in(&fake);
        let storage = resumed(&fake, &account.refresh_token);
        Some((fake, storage))
    }

    mod onedrive_storage {
        use super::*;
        storage_contract_tests!(onedrive_storage);
    }

    #[test]
    fn signing_in_gives_the_account() {
        let fake = FakeMicrosoft::start();

        let account = sign_in(&fake);

        assert_eq!(account.email, EMAIL);
        assert!(!account.refresh_token.is_empty());
    }

    #[test]
    fn the_sign_in_page_asks_for_the_app_folder_scope_only_and_for_personal_accounts() {
        let sign_in = SignIn::start(CLIENT_ID).unwrap();

        assert!(
            sign_in
                .url()
                .starts_with("https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?")
        );
        let scope = format!(
            "scope={}",
            encode("Files.ReadWrite.AppFolder offline_access")
        );
        assert!(sign_in.url().contains(&scope));
        assert_eq!(sign_in.callback_scheme(), "msauth.com.timhilt.pkm");
    }

    #[test]
    fn refusing_consent_is_access_denied_and_touches_nothing() {
        let fake = FakeMicrosoft::start();
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
        let fake = FakeMicrosoft::start();
        let other = SignIn::start_with(fake.hosts(), CLIENT_ID).unwrap();
        let redirect = fake.browser(other.url(), true);
        let sign_in = SignIn::start_with(fake.hosts(), CLIENT_ID).unwrap();

        assert_eq!(
            sign_in.finish(&redirect).unwrap_err(),
            ConnectError::SignInFailed
        );
        assert!(fake.requests().is_empty());
    }

    #[test]
    fn a_work_or_school_account_is_not_supported() {
        let fake = FakeMicrosoft::start();
        fake.make_work_account();
        let sign_in = SignIn::start_with(fake.hosts(), CLIENT_ID).unwrap();
        let redirect = fake.browser(sign_in.url(), true);

        assert_eq!(
            sign_in.finish(&redirect).unwrap_err(),
            ConnectError::UnsupportedAccount
        );
    }

    #[test]
    fn an_unreachable_microsoft_is_unreachable() {
        let fake = FakeMicrosoft::start();
        let sign_in = SignIn::start_with(fake.hosts(), CLIENT_ID).unwrap();
        let redirect = fake.browser(sign_in.url(), true);
        drop(fake);

        assert_eq!(
            sign_in.finish(&redirect).unwrap_err(),
            ConnectError::Unreachable
        );
    }

    #[test]
    fn resuming_contacts_nobody() {
        let fake = FakeMicrosoft::start();
        let _storage = resumed(&fake, "refresh-1");
        assert!(fake.requests().is_empty());
    }

    #[test]
    fn names_with_spaces_quotes_and_non_ascii_work() {
        let (fake, storage) = onedrive_storage().unwrap();
        for name in ["it's #1 ünï.excalidraw", "100% done?.excalidraw"] {
            put_bytes(&storage, name, b"1");
            put_bytes(&storage, name, b"2");
            assert_eq!(get_bytes(&storage, name).unwrap().unwrap(), b"2");
        }
        assert_eq!(names(&storage).len(), 2);
        assert_eq!(fake.files().len(), 2);
    }

    #[test]
    fn listing_follows_all_pages() {
        let (fake, storage) = onedrive_storage().unwrap();
        for i in 0..5 {
            fake.add_file(&format!("D{i}.excalidraw"), b"{}");
        }

        assert_eq!(storage.list().unwrap().len(), 5);
    }

    #[test]
    fn a_big_drawing_goes_up_in_parts_and_is_visible_only_when_whole() {
        let (fake, mut storage) = onedrive_storage().unwrap();
        storage.simple_limit = 10;
        storage.chunk_size = 8;
        let content: Vec<u8> = (0..30).collect();

        put_bytes(&storage, "Big.excalidraw", &content);

        assert_eq!(fake.files()["Big.excalidraw"].data, content);
        let uploads = fake
            .requests()
            .iter()
            .filter(|r| r.starts_with("PUT /session/"))
            .count();
        assert_eq!(uploads, 4);
        // Replacing goes the same way and leaves one blob.
        put_bytes(&storage, "Big.excalidraw", &content[..20]);
        assert_eq!(names(&storage), ["Big.excalidraw"]);
        assert_eq!(
            get_bytes(&storage, "Big.excalidraw").unwrap().unwrap(),
            &content[..20]
        );
    }

    #[test]
    fn a_small_drawing_goes_up_in_one_request() {
        let (fake, storage) = onedrive_storage().unwrap();

        put_bytes(&storage, "Sketch.excalidraw", b"{}");

        let requests = fake.requests();
        assert!(requests.iter().all(|r| !r.contains("createUploadSession")));
        assert_eq!(fake.files()["Sketch.excalidraw"].data, b"{}");
    }

    #[test]
    fn an_expired_access_token_is_refreshed_by_the_crate() {
        let (fake, storage) = onedrive_storage().unwrap();
        put_bytes(&storage, "Sketch.excalidraw", b"{}");
        let before = fake.refreshes();

        fake.expire_access_tokens();

        assert_eq!(names(&storage), ["Sketch.excalidraw"]);
        assert_eq!(fake.refreshes(), before + 1);
        names(&storage);
        assert_eq!(fake.refreshes(), before + 1);
    }

    #[test]
    fn a_rotated_refresh_token_is_handed_over_and_used_from_then_on() {
        let fake = FakeMicrosoft::start();
        let account = sign_in(&fake);
        fake.rotate_refresh_tokens();
        let handed = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&handed);
        let storage = OneDriveStorage::resume_with(
            fake.hosts(),
            CLIENT_ID,
            &account.refresh_token,
            Arc::new(move |token| log.lock().unwrap().push(token.to_owned())),
        );

        storage.list().unwrap();
        let rotated = handed.lock().unwrap().clone();
        assert_eq!(rotated.len(), 1);
        assert_ne!(rotated[0], account.refresh_token);

        fake.expire_access_tokens();
        storage.list().unwrap();
        assert_eq!(handed.lock().unwrap().len(), 2);
        let from_keychain = resumed(&fake, &handed.lock().unwrap()[1]);
        assert!(from_keychain.list().is_ok());
    }

    #[test]
    fn revoked_access_is_permission_denied_not_offline() {
        let (fake, storage) = onedrive_storage().unwrap();
        put_bytes(&storage, "Sketch.excalidraw", b"{}");

        fake.revoke();

        let error = storage.list().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let error = storage.put("x", &mut &b"1"[..]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn a_full_onedrive_is_storage_full_but_reading_goes_on() {
        let (fake, mut storage) = onedrive_storage().unwrap();
        put_bytes(&storage, "Sketch.excalidraw", b"{}");

        fake.fill_up();

        let small = storage
            .put("Sketch.excalidraw", &mut &b"2"[..])
            .unwrap_err();
        assert_eq!(small.kind(), io::ErrorKind::StorageFull);
        storage.simple_limit = 0;
        let big = storage.put("New.excalidraw", &mut &b"1"[..]).unwrap_err();
        assert_eq!(big.kind(), io::ErrorKind::StorageFull);
        assert_eq!(
            get_bytes(&storage, "Sketch.excalidraw").unwrap().unwrap(),
            b"{}"
        );
        storage.delete("Sketch.excalidraw").unwrap();
    }

    #[test]
    fn no_answer_is_not_permission_denied() {
        let fake = FakeMicrosoft::start();
        let storage = resumed(&fake, "refresh-1");
        drop(fake);

        let error = storage.list().unwrap_err();

        assert_ne!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_ne!(error.kind(), io::ErrorKind::StorageFull);
    }
}
