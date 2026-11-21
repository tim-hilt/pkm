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
    /// scheme), its page and Cover downloaded and read. An Address Saved already isn't downloaded.
    /// A download that fails, or none for a PDF or EPUB, leaves the Address alone to be Saved.
    func prepare(_ text: String) async -> Prepared {
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
        var cover: Data?
        if let address = details.cover, let url = URL(string: address) { cover = try? await download(url).data }
        let article = readArticle(html: html, address: address)
        return .draft(SaveDraft(address: address, details: details, cover: cover, article: article, failure: failure))
    }

    /// Saves the page under `title` (blank keeps the page's), with its Article if `keepsArticle`
    /// (the Save sheet's toggle, which starts as `draft.article.readsLikeArticle`); the Bookmark,
    /// or the one Saved before under the same Address.
    @discardableResult
    func save(_ draft: SaveDraft, title: String, keepsArticle: Bool) -> Bookmark? {
        guard
            let saved = try? store.save(
                address: draft.address, details: draft.details, title: title, cover: draft.cover,
                article: keepsArticle ? draft.article.markdown : nil)
        else { return nil }
        reload()
        return saved.bookmark
    }

    /// The Markdown of the Bookmark's Article; nil if it has none.
    func article(of bookmark: Bookmark) -> String? {
        bookmark.hasArticle ? try? store.readArticle(id: bookmark.id) : nil
    }

    /// Deletes the Bookmark with its Cover and Article.
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
