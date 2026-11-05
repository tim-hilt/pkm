import Foundation
import Testing

@testable import WorkingTitle

/// The licences screen lists every third-party work the app ships, links or ports,
/// each with its full licence text from the bundle.
@Suite struct LicensesTests {
    @Test func listsEveryThirdPartyWorkWithItsLicence() {
        let names = ThirdPartyLicense.all.map { "\($0.name) (\($0.license))" }
        #expect(
            names == [
                "rough.js (MIT)", "perfect-freehand (MIT)", "Excalifont (OFL-1.1)", "Excalidraw (MIT)",
                "UniFFI (MPL-2.0)", "Rust standard library, anyhow, bytes (MIT AND Unicode-3.0)",
            ])
    }

    @Test func everyLicenceHasItsFullTextBundled() throws {
        let expected = [
            "rough.js": "Copyright (c) 2019 Preet Shihn",
            "perfect-freehand": "Copyright (c) 2021 Stephen Ruiz Ltd",
            "Excalifont": "SIL OPEN FONT LICENSE Version 1.1",
            "Excalidraw": "Copyright (c) 2020 Excalidraw",
            "UniFFI": "https://github.com/mozilla/uniffi-rs",
            "Rust standard library, anyhow, bytes": "Copyright (c) 2018 Carl Lerche",
        ]
        for entry in ThirdPartyLicense.all {
            let text = try #require(entry.text(), "\(entry.name) has no bundled text")
            #expect(text.contains(try #require(expected[entry.name])), "\(entry.name)")
            #expect(
                ["THE SOFTWARE IS PROVIDED", "THE FONT SOFTWARE IS PROVIDED", "Disclaimer of Warranty"]
                    .contains { text.contains($0) }, "\(entry.name) has its warranty clause")
        }
    }
}
