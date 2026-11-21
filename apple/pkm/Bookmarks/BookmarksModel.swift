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

/// The Bookmarks Context's state: the list, newest Saved first, from the core's store. It downloads,
/// since only a client can, and otherwise forwards to the store; every Bookmarks rule lives in the core.
@MainActor @Observable
final class BookmarksModel {
    private let store: BookmarkStore
    private let download: PageDownloader

    /// Every Bookmark, newest Saved first.
    private(set) var bookmarks: [Bookmark] = []

    /// The Bookmark whose Article the Reader shows, if any.
    var openID: String?

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
        if let cover = details.cover, !wanted.contains(cover) { wanted.append(cover) }
        progress(1 / Double(wanted.count + 1))
        var images: [String: Data] = [:]
        var done = 1
        for await (address, picture) in downloadImages(wanted) {
            if let picture { images[address] = picture }
            done += 1
            progress(Double(done) / Double(wanted.count + 1))
        }
        let cover = details.cover.flatMap { images[$0] }
        return .draft(
            SaveDraft(
                address: address, details: details, cover: cover,
                images: images.filter { article.images.contains($0.key) }, article: article, failure: failure))
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

    /// Saves the page under `title` (blank keeps the page's), with its Article and images if
    /// `keepsArticle` (the Save sheet's toggle, which starts as `draft.article.readsLikeArticle`);
    /// the Bookmark, or the one Saved before under the same Address.
    @discardableResult
    func save(_ draft: SaveDraft, title: String, keepsArticle: Bool) -> Bookmark? {
        guard
            let saved = try? store.save(
                address: draft.address, details: draft.details, title: title, cover: draft.cover,
                article: keepsArticle ? draft.article.markdown : nil, images: keepsArticle ? draft.images : [:])
        else { return nil }
        reload()
        return saved.bookmark
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
    }
}
