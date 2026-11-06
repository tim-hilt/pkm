import Foundation

/// The folder the user granted for iCloud Drive, kept across launches as a
/// security-scoped bookmark: there is no app container (no paid developer
/// membership), so the app reaches iCloud Drive only through this grant.
enum ICloudGrant {
    #if os(macOS)
        private static let creation: URL.BookmarkCreationOptions = .withSecurityScope
        private static let resolution: URL.BookmarkResolutionOptions = .withSecurityScope
    #else
        private static let creation: URL.BookmarkCreationOptions = []
        private static let resolution: URL.BookmarkResolutionOptions = []
    #endif

    /// The bookmark of `folder`, which must be accessible.
    static func bookmark(for folder: URL) throws -> Data {
        try folder.bookmarkData(options: creation, includingResourceValuesForKeys: nil, relativeTo: nil)
    }

    /// The folder `bookmark` stands for, and a renewed bookmark if it went
    /// stale. Nil if the grant is lost: the folder is gone or was revoked.
    static func resolve(_ bookmark: Data) -> (folder: URL, renewed: Data?)? {
        var stale = false
        guard
            let folder = try? URL(
                resolvingBookmarkData: bookmark, options: resolution, relativeTo: nil, bookmarkDataIsStale: &stale)
        else { return nil }
        return (folder, stale ? try? Self.bookmark(for: folder) : nil)
    }
}

/// Asks the system to download files that are in iCloud Drive only.
final class ICloudDownloader: Downloader {
    func download(path: String) {
        try? FileManager.default.startDownloadingUbiquitousItem(at: URL(fileURLWithPath: path))
    }
}
