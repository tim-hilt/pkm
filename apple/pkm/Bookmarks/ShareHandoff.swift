import Foundation

/// How the app and its Share extension pass Bookmarks: the extension is a separate app with a
/// sandbox of its own, so both use a folder in the App Group. The extension Saves into the `inbox`,
/// a Bookmarks folder the app takes from whenever it comes to the front (`Bookmarks.takeFrom`); the
/// app publishes what is Saved and the Tags there are, which the extension can't read otherwise.
struct ShareHandoff {
    static let group = "group.com.timhilt.pkm"

    let folder: URL

    /// The App Group's folder; nil where the app isn't signed with the group.
    static var live: ShareHandoff? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group).map(ShareHandoff.init)
    }

    /// What the app published about one Saved Bookmark: enough to recognise it and offer Add Article.
    struct Entry: Codable, Equatable {
        let address: String
        let title: String
        let hasArticle: Bool
    }

    struct Published: Codable, Equatable {
        var bookmarks: [Entry] = []
        var tags: [String] = []
    }

    private var inboxFolder: URL { folder.appending(path: "Bookmarks", directoryHint: .isDirectory) }
    private var publishedFile: URL { folder.appending(path: "Published.json") }

    func openInbox() throws -> BookmarkStore { try BookmarkStore.open(root: inboxFolder.path) }

    func publish(_ bookmarks: [Bookmark], tags: [String]) {
        let published = Published(
            bookmarks: bookmarks.map { Entry(address: $0.address, title: $0.title, hasArticle: $0.hasArticle) },
            tags: tags)
        guard let data = try? JSONEncoder().encode(published) else { return }
        try? data.write(to: publishedFile, options: .atomic)
    }

    /// What the app last published; nothing before it has.
    func published() -> Published {
        (try? Data(contentsOf: publishedFile)).flatMap { try? JSONDecoder().decode(Published.self, from: $0) }
            ?? Published()
    }
}

extension ShareHandoff.Entry {
    /// As the Bookmark `prepare` reports for an Address Saved in the app.
    var bookmark: Bookmark {
        Bookmark(
            id: "", address: address, title: title, site: "", cover: nil, excerpt: "", author: "", published: "",
            saved: "", hasArticle: hasArticle, tags: [], progress: nil)
    }
}
