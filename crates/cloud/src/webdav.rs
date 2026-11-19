//! The native WebDAV storage: the Cloud Provider for self-hosting (ADR-0012).
//! Any WebDAV server works, e.g. `rclone serve webdav`.
//!
//! The protocol (`webdav_protocol`) says what to send and what the answers
//! mean; this sends it with ureq: HTTPS only, blocking (no async runtime).

use crate::http::tls_config;
use crate::storage::{ConnectError, Storage, Tree, check_path, unique_name};
use crate::webdav_protocol::{Exchange, HttpAnswer, HttpRequest, Turn, WebDav};
use std::fmt;
use std::io::{self, Cursor, Read};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ureq::Agent;
use ureq::http::Request;

/// The folder of a Context in the `pkm` folder of a WebDAV server
/// (`pkm/Drawings`). A rejected login (HTTP 401) is an
/// `io::ErrorKind::PermissionDenied` error, a full server (507) is
/// `StorageFull`, anything else that fails to connect is another kind.
pub struct WebDavStorage {
    agent: Agent,
    protocol: WebDav,
    /// Whether the `.incoming` folder is known to exist.
    incoming_exists: AtomicBool,
}

impl fmt::Debug for WebDavStorage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("WebDavStorage")
            .field("protocol", &self.protocol)
            .finish_non_exhaustive()
    }
}

impl WebDavStorage {
    /// Connects to the WebDAV server at `url` and checks the details. Creates
    /// the `pkm` folder inside `url` and the folder of `context` ("Drawings")
    /// in it if they're missing; uses them if not.
    pub fn connect(
        url: &str,
        username: &str,
        password: &str,
        context: &str,
    ) -> Result<Self, ConnectError> {
        Self::connect_with(url, username, password, context, tls_config())
    }

    /// The storage of a server that was connected before, e.g. at the last
    /// launch. Contacts nobody, so it works offline; errors show on use.
    pub fn resume(
        url: &str,
        username: &str,
        password: &str,
        context: &str,
    ) -> Result<Self, ConnectError> {
        Self::new(url, username, password, context, tls_config())
    }

    fn connect_with(
        url: &str,
        username: &str,
        password: &str,
        context: &str,
        tls: ureq::tls::TlsConfig,
    ) -> Result<Self, ConnectError> {
        let storage = Self::new(url, username, password, context, tls)?;
        storage.run(storage.protocol.connect())?;
        Ok(storage)
    }

    /// The storage for `url`, which must be https, without any request.
    fn new(
        url: &str,
        username: &str,
        password: &str,
        context: &str,
        tls: ureq::tls::TlsConfig,
    ) -> Result<Self, ConnectError> {
        let protocol = WebDav::new(url, username, password, context)?;
        let config = Agent::config_builder()
            .https_only(true)
            // PROPFIND, MKCOL and MOVE.
            .allow_non_standard_methods(true)
            .http_status_as_error(false)
            .tls_config(tls)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .build();
        Ok(Self {
            agent: Agent::new_with_config(config),
            protocol,
            incoming_exists: AtomicBool::new(false),
        })
    }

    /// Sends one request; any answer is returned, whatever its status.
    fn send(&self, request: &HttpRequest) -> io::Result<HttpAnswer> {
        let mut builder = Request::builder().method(request.method).uri(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(*name, value);
        }
        let mut body = request.body.as_slice();
        let built = builder
            .body(ureq::SendBody::from_reader(&mut body))
            .map_err(io::Error::other)?;
        let response = self.agent.run(built).map_err(ureq::Error::into_io)?;
        let status = response.status().as_u16();
        let mut answer = Vec::new();
        response
            .into_body()
            .into_reader()
            .read_to_end(&mut answer)?;
        Ok(HttpAnswer {
            status,
            body: answer,
        })
    }

    /// Runs `exchange` to its result, sending each request it asks for.
    fn run<E: Exchange>(&self, mut exchange: E) -> E::Output {
        let mut request = exchange.start();
        loop {
            let answer = match self.send(&request) {
                Ok(answer) => answer,
                Err(error) => return exchange.failed(error),
            };
            match exchange.answer(answer) {
                Turn::Send(next) => request = next,
                Turn::Done(output) => return output,
            }
        }
    }
}

impl Storage for WebDavStorage {
    fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()> {
        check_path(name)?;
        let mut bytes = Vec::new();
        data.read_to_end(&mut bytes)?;
        // The temporary names of all Devices share one folder.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |t| t.as_nanos());
        let temp = format!("{nanos}-{}", unique_name());
        let known = self.incoming_exists.load(Relaxed);
        let result = self.run(self.protocol.put(name, &temp, bytes, known));
        // Maybe the folder was removed from outside.
        self.incoming_exists.store(result.is_ok(), Relaxed);
        result
    }

    fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
        check_path(name)?;
        let body = self.run(self.protocol.get(name))?;
        Ok(body.map(|bytes| Box::new(Cursor::new(bytes)) as Box<dyn Read + Send>))
    }

    fn scan(&self) -> io::Result<Tree> {
        self.run(self.protocol.scan())
    }

    fn create_dir(&self, path: &str) -> io::Result<()> {
        check_path(path)?;
        self.run(self.protocol.create_dir(path))
    }

    fn delete(&self, name: &str) -> io::Result<()> {
        check_path(name)?;
        self.run(self.protocol.delete(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::contract::{Scratch, storage_contract_tests};
    use crate::webdav_protocol::{FOLDER, basic_auth};
    use std::fs;
    use std::net::TcpListener;
    use std::process::{Child, Command, Stdio};
    use std::thread::sleep;

    const USER: &str = "reader";
    const PASSWORD: &str = "s3cret";

    /// An `rclone serve` process on localhost with a fresh self-signed
    /// certificate, serving a directory. None when rclone or openssl isn't
    /// installed, so tests skip. The certificate is short-lived because Apple's
    /// TLS refuses long validity periods.
    struct Rclone {
        child: Child,
        url: String,
        cert: String,
        dir: Scratch,
        _certs: Scratch,
    }

    impl Rclone {
        fn start(protocol: &str) -> Option<Self> {
            let installed = |tool: &str, arg: &str| {
                Command::new(tool)
                    .arg(arg)
                    .stdout(Stdio::null())
                    .status()
                    .is_ok_and(|status| status.success())
            };
            if !installed("rclone", "version") || !installed("openssl", "version") {
                eprintln!("skipped: rclone or openssl is not installed");
                return None;
            }
            let port = TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
            let dir = Scratch::new("webdav-served");
            let certs = Scratch::new("webdav-certs");
            let cert = certs.0.join("localhost.crt");
            let key = certs.0.join("localhost.key");
            let made = Command::new("openssl")
                .args(["req", "-x509", "-newkey", "ec", "-pkeyopt"])
                .args(["ec_paramgen_curve:prime256v1", "-nodes", "-days", "30"])
                .args(["-subj", "/CN=localhost"])
                .args(["-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1"])
                .args(["-addext", "extendedKeyUsage=serverAuth"])
                .arg("-keyout")
                .arg(&key)
                .arg("-out")
                .arg(&cert)
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(made.success(), "openssl could not make a certificate");
            let child = Command::new("rclone")
                .args(["serve", protocol])
                .arg(&dir.0)
                .args(["--addr", &format!("127.0.0.1:{port}")])
                .args(["--user", USER, "--pass", PASSWORD])
                .arg("--cert")
                .arg(&cert)
                .arg("--key")
                .arg(&key)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let server = Self {
                child,
                url: format!("https://127.0.0.1:{port}"),
                cert: fs::read_to_string(&cert).unwrap(),
                dir,
                _certs: certs,
            };
            for _ in 0..200 {
                if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                    return Some(server);
                }
                sleep(Duration::from_millis(25));
            }
            panic!("rclone did not start");
        }

        fn webdav() -> Option<Self> {
            Self::start("webdav")
        }

        fn connect(&self) -> Result<WebDavStorage, ConnectError> {
            self.connect_as(USER, PASSWORD)
        }

        fn connect_as(&self, user: &str, pass: &str) -> Result<WebDavStorage, ConnectError> {
            let tls = trusting(&self.cert);
            WebDavStorage::connect_with(&self.url, user, pass, "Drawings", tls)
        }

        fn stop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl Drop for Rclone {
        fn drop(&mut self) {
            self.stop();
        }
    }

    fn trusting(cert: &str) -> ureq::tls::TlsConfig {
        let der = pem_to_der(cert);
        let roots = ureq::tls::RootCerts::new_with_certs(&[
            ureq::tls::Certificate::from_der(&der).to_owned()
        ]);
        ureq::tls::TlsConfig::builder()
            .provider(tls_config().provider())
            .root_certs(roots)
            .build()
    }

    /// The DER bytes of the one certificate in `pem`.
    fn pem_to_der(pem: &str) -> Vec<u8> {
        let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
        let mut out = Vec::new();
        let mut bits = 0u32;
        let mut count = 0;
        for c in body.bytes().filter(|c| *c != b'=') {
            let value = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                _ => 63,
            };
            bits = bits << 6 | u32::from(value);
            count += 6;
            if count >= 8 {
                count -= 8;
                out.push((bits >> count) as u8);
            }
        }
        out
    }

    fn webdav_storage() -> Option<(Rclone, WebDavStorage)> {
        let server = Rclone::webdav()?;
        let storage = server.connect().unwrap();
        Some((server, storage))
    }

    mod webdav_storage {
        use super::*;
        storage_contract_tests!(webdav_storage);
    }

    #[test]
    fn connecting_needs_an_https_url() {
        for url in [
            "http://example.com",
            "example.com",
            "ftp://x",
            "",
            "https:/x",
        ] {
            let error = WebDavStorage::connect(url, "u", "p", "Drawings").unwrap_err();
            assert_eq!(error, ConnectError::NotHttps, "{url:?}");
        }
    }

    #[test]
    fn resuming_needs_an_https_url_but_contacts_no_server() {
        assert_eq!(
            WebDavStorage::resume("http://example.com", "u", "p", "Drawings").unwrap_err(),
            ConnectError::NotHttps
        );
        // Nobody listens there: resuming works offline, using it fails.
        let storage = WebDavStorage::resume("https://127.0.0.1:1", "u", "p", "Drawings").unwrap();
        assert!(storage.list().is_err());
    }

    #[test]
    fn connecting_to_nothing_is_unreachable() {
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let url = format!("https://127.0.0.1:{port}");
        let error = WebDavStorage::connect(&url, "u", "p", "Drawings").unwrap_err();
        assert_eq!(error, ConnectError::Unreachable);
    }

    #[test]
    fn connecting_with_a_wrong_password_or_user_is_wrong_credentials() {
        let Some(server) = Rclone::webdav() else {
            return;
        };
        for (user, pass) in [(USER, "nope"), ("nobody", PASSWORD), ("", "")] {
            let error = server.connect_as(user, pass).unwrap_err();
            assert_eq!(error, ConnectError::WrongCredentials, "{user:?}");
        }
    }

    #[test]
    fn connecting_to_a_server_that_is_no_webdav_server_is_not_webdav() {
        let Some(server) = Rclone::start("http") else {
            return;
        };
        assert_eq!(server.connect().unwrap_err(), ConnectError::NotWebDav);
    }

    #[test]
    fn a_certificate_nobody_vouches_for_is_unreachable() {
        let Some(server) = Rclone::webdav() else {
            return;
        };
        let error = WebDavStorage::connect(&server.url, USER, PASSWORD, "Drawings").unwrap_err();
        assert_eq!(error, ConnectError::Unreachable);
    }

    #[test]
    fn connecting_creates_the_context_folder_and_reuses_it() {
        let Some(server) = Rclone::webdav() else {
            return;
        };
        let folder = server.dir.0.join(FOLDER).join("Drawings");
        assert!(!folder.exists());

        let first = server.connect().unwrap();
        assert!(folder.is_dir());
        first.put("blob", &mut &b"kept"[..]).unwrap();

        let second = server.connect().unwrap();
        assert_eq!(
            crate::storage::get_bytes(&second, "blob").unwrap().unwrap(),
            b"kept"
        );
        assert_eq!(fs::read(folder.join("blob")).unwrap(), b"kept");
    }

    #[test]
    fn a_context_added_after_connecting_gets_its_folder_on_the_first_put() {
        let Some(server) = Rclone::webdav() else {
            return;
        };
        server.connect().unwrap();
        // An update brings Tasks; the connection is resumed, not made again.
        let tls = trusting(&server.cert);
        let tasks = WebDavStorage::new(&server.url, USER, PASSWORD, "Tasks", tls).unwrap();
        tasks.put("task.md", &mut &b"kept"[..]).unwrap();
        let folder = server.dir.0.join(FOLDER).join("Tasks");
        assert_eq!(fs::read(folder.join("task.md")).unwrap(), b"kept");
    }

    #[test]
    fn a_url_with_a_path_keeps_everything_inside_it() {
        let Some(server) = Rclone::webdav() else {
            return;
        };
        fs::create_dir(server.dir.0.join("my files")).unwrap();

        let url = format!("{}/my%20files/", server.url);
        let storage =
            WebDavStorage::connect_with(&url, USER, PASSWORD, "Drawings", trusting(&server.cert))
                .unwrap();
        storage.put("blob", &mut &b"x"[..]).unwrap();

        assert!(server.dir.0.join("my files/pkm/Drawings/blob").is_file());
    }

    #[test]
    fn rejected_credentials_during_use_differ_from_being_offline() {
        let Some((mut server, mut storage)) = webdav_storage() else {
            return;
        };
        storage.put("blob", &mut &b"x"[..]).unwrap();

        // The password changed on the server.
        storage.protocol.auth = basic_auth(USER, "changed");
        for error in [
            storage.list().unwrap_err(),
            storage.get("blob").map(|_| ()).unwrap_err(),
            storage.put("other", &mut &b"y"[..]).unwrap_err(),
            storage.delete("blob").unwrap_err(),
        ] {
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        }

        // The server went away.
        storage.protocol.auth = basic_auth(USER, PASSWORD);
        server.stop();
        let error = storage.list().unwrap_err();
        assert_ne!(error.kind(), io::ErrorKind::PermissionDenied);
    }
}
