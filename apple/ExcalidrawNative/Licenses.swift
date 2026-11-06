import SwiftUI

/// A third-party work the app ships (Excalifont), links (the Rust crates in
/// the core's static library) or ports (rough.js, perfect-freehand, Excalidraw
/// as the reference implementation), with its licence text bundled as a
/// resource. A new Rust dependency that ends up in the app binary needs an
/// entry here (`nm` on the app's dSYM lists the crates).
/// Code that only runs at build time (the bindings generator) does not.
struct ThirdPartyLicense: Identifiable, Sendable {
    let name: String
    /// SPDX expression, shown next to the name.
    let license: String
    /// Name of the bundled `.txt` resource holding the full licence text.
    let resource: String

    var id: String { name }

    static let all = [
        ThirdPartyLicense(name: "rough.js", license: "MIT", resource: "LICENSE-roughjs"),
        ThirdPartyLicense(name: "perfect-freehand", license: "MIT", resource: "LICENSE-perfect-freehand"),
        ThirdPartyLicense(name: "Excalifont", license: "OFL-1.1", resource: "OFL"),
        ThirdPartyLicense(name: "Excalidraw", license: "MIT", resource: "LICENSE-excalidraw"),
        ThirdPartyLicense(name: "UniFFI", license: "MPL-2.0", resource: "LICENSE-uniffi"),
        ThirdPartyLicense(
            name: "Rust standard library, anyhow, bytes", license: "MIT AND Unicode-3.0", resource: "LICENSE-rust"),
    ]

    private final class BundleToken {}

    /// The full licence text, or nil when the resource is missing.
    func text(in bundle: Bundle = Bundle(for: BundleToken.self)) -> String? {
        guard let url = bundle.url(forResource: resource, withExtension: "txt") else { return nil }
        return try? String(contentsOf: url, encoding: .utf8)
    }
}

/// Lists the third-party licences; each row opens the full text.
struct LicensesView: View {
    var body: some View {
        List(ThirdPartyLicense.all) { entry in
            NavigationLink {
                ScrollView {
                    Text(entry.text() ?? "")
                        .font(.footnote.monospaced())
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding()
                }
                .navigationTitle(entry.name)
            } label: {
                LabeledContent(entry.name, value: entry.license)
            }
            .accessibilityIdentifier("license-\(entry.name)")
        }
        .navigationTitle("Licences")
    }
}
