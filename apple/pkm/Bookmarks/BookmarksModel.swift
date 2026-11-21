import CoreGraphics
import Foundation
import ImageIO
import Observation

/// A page ready to Save: what the core made of the download, for the Save sheet to show.
struct SaveDraft: Equatable {
    /// Normalised.
    let address: String
    let details: Details
    let cover: Data?
    /// The downloaded images of `article`, by the address it names them with; one that failed to
    /// download is missing, which drops it from the Article when Saved.
    let images: [String: Data]
    /// The page's text, empty where there is none, and whether it reads like an Article.
    let article: Article
    /// Why the page couldn't be read, if it couldn't; the Bookmark is then Saved with its Address only.
    let failure: String?
}

/// What preparing an Address came to.
enum Prepared: Equatable {
    case invalid
    /// Its Address is Saved already.
    case existing(Bookmark)
    case draft(SaveDraft)
}

/// The Bookmarks Context's state: the list, newest Saved first, narrowed to a Tag if one is chosen, from the core's store. It downloads,
/// since only a client can, and otherwise forwards to the store; every Bookmarks rule lives in the core.
@MainActor @Observable
final class BookmarksModel {
    private let store: BookmarkStore
    private let download: PageDownloader

    /// Every Bookmark, newest Saved first.
    private(set) var bookmarks: [Bookmark] = []

    /// The Tag the list is narrowed to, if any.
    var tagFilter: String? {
        didSet { reload() }
    }

    /// The Bookmarks the list shows: those carrying `tagFilter`, or all.
    private(set) var shown: [Bookmark] = []

    /// The Tags some Bookmark carries, alphabetically: what the list can be narrowed to.
    var tagsInUse: [String] { Set(bookmarks.flatMap(\.tags)).sorted() }

    /// The Bookmark whose Article the Reader shows, if any.
    var openID: String?

    /// The Bookmark whose Details are shown, if any.
    var detailsID: String?

    /// A Link to a Bookmark: its Details over the list, with no Reader open. One that is gone leaves
    /// just the list; `nil` does the same.
    func openLinked(id: String?) {
        openID = nil
        detailsID = id.flatMap { id in bookmarks.contains { $0.id == id } ? id : nil }
    }

    /// What is pushed over the list: the Reader of `openID`.
    var pushedPath: [String] {
        get { openID.map { [$0] } ?? [] }
        set { openID = newValue.last }
    }

    /// The Covers shown, decoded once.
    @ObservationIgnored private var covers: [String: CGImage] = [:]

    init(store: BookmarkStore, download: @escaping PageDownloader = Download.live) {
        self.store = store
        self.download = download
        reload()
    }

    /// The Save sheet's first step: the Address as typed (`https://` is assumed if it names no
    /// scheme), its page, Cover and the Article's images downloaded and read. An Address Saved
    /// already isn't downloaded. A download that fails, or none for a PDF or EPUB, leaves the Address
    /// alone to be Saved; an image that fails is left out. `progress` is called with the share of
    /// downloads done, from the page on.
    func prepare(_ text: String, progress: (Double) -> Void = { _ in }) async -> Prepared {
        let typed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let address = normaliseAddress(address: typed.contains("://") ? typed : "https://" + typed),
            let url = URL(string: address)
        else { return .invalid }
        if let existing = bookmarks.first(where: { $0.address == address }) { return .existing(existing) }
        return .draft(await fetch(address, url: url, withCover: true, progress: progress))
    }

    /// The page at `address` (`url`), its Cover if `withCover` and the Article's images downloaded
    /// and read, as `prepare` describes.
    private func fetch(
        _ address: String, url: URL, withCover: Bool, progress: (Double) -> Void = { _ in }
    ) async -> SaveDraft {
        var html = ""
        var failure: String?
        if !isDocumentAddress(address: address) {
            do {
                guard let page = try await download(url).html else { throw PageDownloadError.notAPage }
                html = page
            } catch {
                failure = error.localizedDescription
            }
        }
        let details = extractDetails(html: html, address: address)
        let article = readArticle(html: html, address: address)
        var wanted = article.images
        if withCover, let cover = details.cover, !wanted.contains(cover) { wanted.append(cover) }
        progress(1 / Double(wanted.count + 1))
        var images: [String: Data] = [:]
        var done = 1
        for await (address, picture) in downloadImages(wanted) {
            if let picture { images[address] = picture }
            done += 1
            progress(Double(done) / Double(wanted.count + 1))
        }
        let cover = withCover ? details.cover.flatMap { images[$0] } : nil
        return SaveDraft(
            address: address, details: details, cover: cover,
            images: images.filter { article.images.contains($0.key) }, article: article, failure: failure)
    }

    /// Downloads the images at `addresses`, a few at a time, as they finish, each stored as the
    /// core keeps images (ADR-0029); nil for one that fails or is no picture.
    private func downloadImages(_ addresses: [String]) -> AsyncStream<(String, Data?)> {
        let download = download
        return AsyncStream { continuation in
            let work = Task {
                await withTaskGroup(of: (String, Data?).self) { group in
                    var pending = addresses.makeIterator()
                    func next() {
                        guard let address = pending.next() else { return }
                        group.addTask {
                            guard let url = URL(string: address), let data = try? await download(url).data
                            else { return (address, nil) }
                            return (address, StoredImage.bytes(of: data))
                        }
                    }
                    for _ in 0..<Self.parallelDownloads { next() }
                    for await done in group {
                        continuation.yield(done)
                        next()
                    }
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in work.cancel() }
        }
    }

    private static let parallelDownloads = 6

    /// Saves the page under `title` (blank keeps the page's) and `tags`, with its Article and images if
    /// `keepsArticle` (the Save sheet's toggle, which starts as `draft.article.readsLikeArticle`);
    /// the Bookmark, or the one Saved before under the same Address, which keeps its own Tags.
    @discardableResult
    func save(_ draft: SaveDraft, title: String, tags: [String] = [], keepsArticle: Bool) -> Bookmark? {
        guard
            let saved = try? store.save(
                address: draft.address, details: draft.details, title: title, cover: draft.cover,
                article: keepsArticle ? draft.article.markdown : nil, images: keepsArticle ? draft.images : [:])
        else { return nil }
        if !saved.existing { setTags(tags, of: saved.bookmark.id) }
        reload()
        return bookmarks.first { $0.id == saved.bookmark.id } ?? saved.bookmark
    }

    /// Renames the Bookmark; a blank `title` changes nothing.
    func rename(id: String, to title: String) {
        _ = try? store.rename(id: id, title: title)
        reload()
    }

    /// Gives the Bookmark exactly `tags`: those it lacks are added in order, the others removed. Tags
    /// the app didn't know become known.
    func setTags(_ tags: [String], of id: String) {
        guard let current = try? store.read(id: id).tags else { return }
        for tag in current where !tags.contains(tag) { _ = try? store.removeTag(id: id, tag: tag) }
        for tag in tags where !current.contains(tag) { _ = try? store.addTag(id: id, tag: tag) }
        reload()
    }

    /// Takes `tag` off every Bookmark, as part of Delete a Tag.
    func removeTagEverywhere(_ tag: String) {
        try? store.removeTagEverywhere(tag: tag)
        if tagFilter == tag { tagFilter = nil }
        reload()
    }

    /// Adds the Article of the page as it is now to a Bookmark without one. False if the page can't
    /// be downloaded or has no text, and nothing changes.
    func addArticle(to bookmark: Bookmark) async -> Bool {
        guard !bookmark.hasArticle, let url = URL(string: bookmark.address) else { return false }
        let page = await fetch(bookmark.address, url: url, withCover: false)
        guard page.failure == nil, !page.article.markdown.isEmpty,
            (try? store.addArticle(id: bookmark.id, article: page.article.markdown, images: page.images)) != nil
        else { return false }
        reload()
        return true
    }

    /// Takes the Article, with its images, off the Bookmark, which closes its Reader.
    func dropArticle(id: String) {
        if openID == id { openID = nil }
        _ = try? store.dropArticle(id: id)
        reload()
    }

    /// The Markdown of the Bookmark's Article; nil if it has none.
    func article(of bookmark: Bookmark) -> String? {
        bookmark.hasArticle ? try? store.readArticle(id: bookmark.id) : nil
    }

    /// The image `name` of the Bookmark's Article, nil if it can't be read.
    func image(_ name: String, of id: String) -> Data? {
        try? store.readImage(id: id, name: name)
    }

    /// Deletes the Bookmark with its Cover, Article and images.
    func delete(id: String) {
        if openID == id { openID = nil }
        if detailsID == id { detailsID = nil }
        try? store.delete(id: id)
        covers[id] = nil
        reload()
    }

    /// The Cover of the Bookmark, at most 240 px wide so that a list of them stays light.
    func cover(of bookmark: Bookmark) -> CGImage? {
        guard bookmark.cover != nil else { return nil }
        if let known = covers[bookmark.id] { return known }
        guard let data = try? store.readCover(id: bookmark.id),
            let source = CGImageSourceCreateWithData(data as CFData, nil)
        else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: 240,
        ]
        let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
        covers[bookmark.id] = image
        return image
    }

    private func reload() {
        bookmarks = (try? store.bookmarks()) ?? []
        shown = tagFilter.flatMap { try? store.bookmarksTagged(tag: $0) } ?? bookmarks
    }
}

extension Bookmark: Identifiable {}
