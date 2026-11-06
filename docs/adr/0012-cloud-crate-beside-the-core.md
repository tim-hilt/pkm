# Cloud Providers live in their own crate beside the core

HTTP, OAuth and the four Cloud Providers (WebDAV, iCloud Drive, Google Drive, OneDrive) are in a new Rust crate, `crates/cloud`, exposed through `crates/bindings`, so the core keeps its dependency surface to nothing (ADR-0009) and Android and Linux get the same code later (ADR-0001). The code is copied from Leaf and adapted rather than shared, since Drawings are overwritten where Leaf's files never are, and a shared crate would tie the two apps' releases together. As in Leaf: no server, polling only; app-scoped permissions (`drive.file`, `Files.ReadWrite.AppFolder`); iCloud Drive through a one-time grant of its root; WebDAV over https only; the system's TLS on Apple platforms.

## Considered Options

- **In the core, as in Leaf**: breaks ADR-0009's intent.
- **Swift with URLSession**: smaller, but every platform would write it again.
