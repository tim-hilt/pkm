# Rust core shared across clients

All platform-independent logic (Scene model, `.excalidraw` (de)serialization, geometry, hit testing, the hand-drawn shape generator, editing actions) lives in a Rust core, with thin native clients on top. The first client is Swift/SwiftUI for iOS, iPadOS and macOS; Android and Linux clients are planned, so a Swift-only core would have to be rewritten per platform.
