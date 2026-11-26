import Foundation
import UniformTypeIdentifiers

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

    /// What the Share sheet handed over: the page of a browser (`ShareExtension.js`'s results),
    /// else the URL of any other app.
    @MainActor init?(items: [NSExtensionItem]) async {
        let providers = items.flatMap { $0.attachments ?? [] }
        if let provider = providers.first(where: {
            $0.hasItemConformingToTypeIdentifier(UTType.propertyList.identifier)
        }),
            let data = await provider.data(for: .propertyList),
            // The browser hands over a dictionary, whose data is its keyed archive.
            let item = try? NSKeyedUnarchiver.unarchivedObject(
                ofClasses: [NSDictionary.self, NSString.self, NSNumber.self, NSArray.self], from: data)
                as? [String: Any],
            let results = item[NSExtensionJavaScriptPreprocessingResultsKey] as? [String: Any],
            let page = SharedPage(preprocessed: results)
        {
            self = page
            return
        }
        guard let provider = providers.first(where: { $0.hasItemConformingToTypeIdentifier(UTType.url.identifier) }),
            let url = await provider.url()
        else { return nil }
        self.init(address: url.absoluteString)
    }
}

extension NSItemProvider {
    @MainActor fileprivate func data(for type: UTType) async -> Data? {
        await withCheckedContinuation { c in
            _ = loadDataRepresentation(for: type) { data, _ in c.resume(returning: data) }
        }
    }

    @MainActor fileprivate func url() async -> URL? {
        await withCheckedContinuation { c in
            _ = loadObject(ofClass: URL.self) { url, _ in c.resume(returning: url) }
        }
    }
}
