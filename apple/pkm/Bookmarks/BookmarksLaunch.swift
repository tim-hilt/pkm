import Foundation

extension BookmarksModel {
    /// The Bookmarks folder in the app's Documents, which Files and Finder show.
    static let documentsFolder = URL.documentsDirectory.appending(path: "Bookmarks", directoryHint: .isDirectory)

    /// The Bookmarks for this launch: the Bookmarks folder. For UI tests and screenshots (`-UITesting…`)
    /// it is temporary, with sample Bookmarks when launched with `-UITestingOpenBookmarks`.
    @MainActor
    static func forLaunch() -> BookmarksModel {
        let arguments = ProcessInfo.processInfo.arguments
        guard arguments.contains(where: { $0.hasPrefix("-UITesting") }) else {
            return BookmarksModel(store: openOrTemporary(documentsFolder))
        }
        let store = openOrTemporary(temporaryFolder())
        if arguments.contains("-UITestingOpenBookmarks") {
            let samples = [
                ("https://example.com/rust", "Rust in Action", "Example Blog"),
                ("https://example.org/design", "A Philosophy of Software Design", "example.org"),
                (
                    "https://example.net/long", "A page whose title is long enough to need two lines in a narrow row",
                    "Example News"
                ),
                ("https://example.dev/offline", "https://example.dev/offline", "example.dev"),
            ]
            for (address, title, site) in samples.reversed() {
                let details = Details(title: title, site: site, cover: nil, excerpt: "", author: "", published: "")
                _ = try? store.save(address: address, details: details, title: "", cover: nil)
            }
        }
        return BookmarksModel(store: store)
    }

    private static func temporaryFolder() -> URL {
        FileManager.default.temporaryDirectory.appending(
            path: "pkmUITests-\(UUID().uuidString)/Bookmarks", directoryHint: .isDirectory)
    }

    /// The store in `folder`, created if needed. Where that fails, a temporary one: the app still
    /// runs, its Bookmarks just don't stay.
    private static func openOrTemporary(_ folder: URL) -> BookmarkStore {
        (try? BookmarkStore.open(root: folder.path)) ?? (try! BookmarkStore.open(root: temporaryFolder().path))
    }
}
