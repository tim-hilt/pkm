# The Sync logic does no I/O, so the web app runs the same code

Cloud Backing in the web app (WebDAV only) reuses `crates/cloud` instead of porting it to TypeScript. The browser has no blocking file or network calls and no threads, so the crate's logic is separated from its I/O: the Sync engine becomes a step machine that asks its caller to list the cloud folder, read, write, rename or delete a Drawing, download or upload a blob, or save Sync state, and is handed the results; the Sync loop's schedule and status were already pure; the WebDAV storage splits into the protocol (requests to send, the `.incoming` upload-then-move, reading the `PROPFIND` listing with quick-xml, mapping answers to errors) and sending. Each platform supplies only the I/O: the native apps blocking file access and ureq, the web app the Origin Private File System and `fetch`. ureq, TLS and `libc` sit behind a default `native` feature, and `crates/web` depends on `crates/cloud` without it, amending ADR-0014 ("depends on the core only"). How a client tells that a local file changed (the native app's inode, modification time and size) becomes an opaque stamp the client supplies.

## Considered Options

- **Port the Sync engine, loop and WebDAV storage to TypeScript, calling the core's merge through wasm**: the quickest start, but the rules that decide whether work is kept (edit wins over delete, healing lost writes, "Sketch 2") would exist twice and drift, and the Sync tests would have to be written again.
- **Move only the per-Drawing rules into the core**: keeps ADR-0014 intact, but leaves the orchestration around them (when to read, what to check before writing, progress) to be written twice.
- **Run the blocking crate in wasm through JavaScript Promise Integration or synchronous requests in a worker**: JSPI is not in Safari, and OPFS directory access is async even in workers.

## Consequences

- The native Sync engine is rewritten on top of the step machine; its existing integration tests and the rclone tests guard that its behaviour stays the same.
- The wasm grows by the Sync engine and quick-xml; the production build's `.wasm` went from 523,823 bytes to 683,484 bytes (+159,661, +30 %), 208,060 to 263,541 bytes with `gzip -9` (+55,481, +27 %), measured with `make web-wasm` and `bun run build` before and after the feature.
- The web app talks to the WebDAV server straight from the browser, so the server must allow the web app's origin (CORS), e.g. `rclone serve webdav --allow-origin https://<web app host>`. A server that doesn't (stock Nextcloud) can't be used from the web app, and a refused request looks to the app like an unreachable server.
- The web app has no keychain: WebDAV credentials are kept in the site's storage, beside the Drawings.
