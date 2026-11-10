# Cloud Providers live in their own crate beside the core

HTTP, OAuth and the four Cloud Providers (WebDAV, iCloud Drive, Google Drive, OneDrive) are in a new Rust crate, `crates/cloud`, exposed through `crates/bindings`, so the core keeps its dependency surface to nothing (ADR-0009) and Android and Linux get the same code later (ADR-0001). The code is copied from an earlier reader app and adapted rather than shared, since Drawings are overwritten where Book Files never are, and a shared crate would tie the two apps' releases together. As there: no server, polling only; app-scoped permissions (`drive.file`, `Files.ReadWrite.AppFolder`); iCloud Drive through a one-time grant of its root; WebDAV over https only; the system's TLS on Apple platforms.

## Considered Options

- **In the core, as in the earlier app**: breaks ADR-0009's intent.
- **Swift with URLSession**: smaller, but every platform would write it again.

## Consequences

- Release build of the macOS app executable, before → after the crate (WebDAV storage, directory storage, ureq on the system's TLS) and the Settings screen: 3,998,704 → 5,099,888 bytes (+1,101,184, +27.5%). Almost all of it is ureq, `native-tls` and the XML parser; the OAuth providers and the Sync engine will add to it. Measured with `xcodebuild -configuration Release -destination 'platform=macOS'`, as the executable inside the `.app`.
- On Apple platforms TLS is the Security framework's and the Device's trust store, so a certificate from a CA the Device trusts works. The framework refuses a certificate valid longer than 825 days or without the `serverAuth` extended key usage, so self-hosters issue theirs accordingly. The WebDAV tests generate a 30-day certificate with `openssl` and serve it with `rclone serve webdav`; they skip without either tool.
- rclone cannot list a folder while a file in it is being written, so WebDAV uploads go to a hidden `.incoming` folder inside `pkm` and are moved into place.
