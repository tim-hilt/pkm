import Foundation

/// What a download gave.
struct Download: Sendable {
    let data: Data
    let mimeType: String?
    let textEncoding: String?
}

/// Downloads an Address: `URLSession` in the app, a stub in tests. The core decides what the
/// bytes are worth; this only fetches them.
typealias PageDownloader = @Sendable (URL) async throws -> Download

enum PageDownloadError: LocalizedError {
    case notAPage
    case tooLarge

    var errorDescription: String? {
        switch self {
        case .notAPage: String(localized: "The address is not a web page.")
        case .tooLarge: String(localized: "The page is too large.")
        }
    }
}

extension Download {
    /// The most a page or image may take, so that a wrong Address isn't read and kept. It is checked
    /// once the download is in, so it bounds what is kept, not the download itself.
    static let limit = 10_000_000

    /// Downloads with `URLSession`, failing for an error status or too much data.
    static let live: PageDownloader = { url in
        var request = URLRequest(url: url, timeoutInterval: 20)
        request.setValue("text/html,application/xhtml+xml,image/*;q=0.8", forHTTPHeaderField: "Accept")
        let (data, response) = try await URLSession.shared.data(for: request)
        if let status = (response as? HTTPURLResponse)?.statusCode, !(200..<300).contains(status) {
            throw URLError(.badServerResponse)
        }
        guard data.count <= limit else { throw PageDownloadError.tooLarge }
        return Download(data: data, mimeType: response.mimeType, textEncoding: response.textEncodingName)
    }

    /// The page's HTML as text, in the encoding the server named, else UTF-8; nil for any other
    /// type of download.
    var html: String? {
        guard let mimeType, ["text/html", "application/xhtml+xml"].contains(mimeType) else { return nil }
        let encoding = textEncoding.map {
            CFStringConvertEncodingToNSStringEncoding(CFStringConvertIANACharSetNameToEncoding($0 as CFString))
        }
        return encoding.flatMap { String(data: data, encoding: String.Encoding(rawValue: $0)) }
            ?? String(decoding: data, as: UTF8.self)
    }
}
