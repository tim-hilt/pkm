import Foundation
import Testing

@testable import WorkingTitle

/// The licences screen lists every third-party work the app ships or ports,
/// each with its full licence text from the bundle.
@Suite struct LicensesTests {
    @Test func listsEveryThirdPartyWorkWithItsLicence() {
        let names = ThirdPartyLicense.all.map { "\($0.name) (\($0.license))" }
        #expect(
            names == [
                "rough.js (MIT)", "perfect-freehand (MIT)", "Excalifont (OFL-1.1)", "Excalidraw (MIT)",
            ])
    }

    @Test func everyLicenceHasItsFullTextBundled() throws {
        let expected = [
            "rough.js": "Copyright (c) 2019 Preet Shihn",
            "perfect-freehand": "Copyright (c) 2021 Stephen Ruiz Ltd",
            "Excalifont": "SIL OPEN FONT LICENSE Version 1.1",
            "Excalidraw": "Copyright (c) 2020 Excalidraw",
        ]
        for entry in ThirdPartyLicense.all {
            let text = try #require(entry.text(), "\(entry.name) has no bundled text")
            #expect(text.contains(try #require(expected[entry.name])), "\(entry.name)")
            #expect(text.contains("THE SOFTWARE IS PROVIDED") || text.contains("THE FONT SOFTWARE IS PROVIDED"))
        }
    }
}
