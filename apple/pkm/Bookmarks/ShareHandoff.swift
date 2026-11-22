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
        let files = FileManager.default
        // The extension lives in the app's `PlugIns`, and may be signed with the app's profile.
        let bundles = [
            Bundle.main.bundleURL, Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent(),
        ]
        let listed = bundles.flatMap { bundle in
            (try? Data(contentsOf: bundle.appending(path: "embedded.mobileprovision"))).map(groups(inProfile:)) ?? []
        }
        return group(listed: listed) { files.containerURL(forSecurityApplicationGroupIdentifier: $0) != nil }
            .flatMap(files.containerURL(forSecurityApplicationGroupIdentifier:)).map(ShareHandoff.init)
    }

    /// The App Group this build may use: the built one, else the one a sideloading tool (AltStore,
    /// SideStore) renamed it to for its team, `<group>.<team>`, which its profile lists.
    static func group(listed: [String], isGranted: (String) -> Bool) -> String? {
        ([group] + listed.filter { $0.hasPrefix(group + ".") }).first(where: isGranted)
    }

    /// The App Groups a provisioning profile grants. Its plist sits inside a signed envelope, which
    /// iOS has no public API to open, so it is cut out between `<?xml` and `</plist>`.
    static func groups(inProfile profile: Data) -> [String] {
        guard let start = profile.firstRange(of: Data("<?xml".utf8)),
            let end = profile.firstRange(of: Data("</plist>".utf8), in: start.lowerBound..<profile.endIndex),
            let plist = try? PropertyListSerialization.propertyList(
                from: profile[start.lowerBound..<end.upperBound], format: nil) as? [String: Any],
            let entitlements = plist["Entitlements"] as? [String: Any]
        else { return [] }
        return entitlements["com.apple.security.application-groups"] as? [String] ?? []
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
