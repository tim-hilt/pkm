import Foundation

/// What a Share sheet hands the extension: the Address, and the page's HTML as the user sees it
/// when a browser shared it. Without HTML (a URL from another app, or a page too large to take
/// along) the page is downloaded.
struct SharedPage: Equatable {
    let address: String
    let html: String?

    /// From the results of `ShareExtension.js`, which runs in the browser's page: its `url` and
    /// `html`. Nil without a `url`; HTML beyond `Download.limit` is left out, to stay within the
    /// extension's memory.
    init?(preprocessed results: [AnyHashable: Any]) {
        guard let address = results["url"] as? String, !address.isEmpty else { return nil }
        self.address = address
        let html = results["html"] as? String
        self.html = html.flatMap { $0.utf8.count <= Download.limit ? $0 : nil }
    }

    init(address: String, html: String? = nil) {
        self.address = address
        self.html = html
    }
}
