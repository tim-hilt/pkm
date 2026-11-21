import Foundation
import Testing

@testable import pkm

@MainActor @Suite struct BookmarksModelTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: UUID().uuidString, directoryHint: .isDirectory)

    private static let page = Download(
        data: Data(
            #"<title>Hello</title><meta property="og:site_name" content="The Site"><meta property="og:image" content="/c.png">"#
                .utf8),
        mimeType: "text/html", textEncoding: "utf-8")
    private static let png = Download(
        data: TestImages.quadrants(), mimeType: "image/png", textEncoding: nil)

    func model(_ download: @escaping PageDownloader) throws -> BookmarksModel {
        BookmarksModel(store: try BookmarkStore.open(root: root.appending(path: "Bookmarks").path), download: download)
    }

    func serving(_ pages: [String: Download]) -> PageDownloader {
        { url in
            guard let found = pages[url.absoluteString] else { throw URLError(.cannotConnectToHost) }
            return found
        }
    }

    @Test func aSavedPageIsListedWithTheTitleTheUserGave() async throws {
        let model = try model(
            serving(["https://example.com/a": Self.page, "https://example.com/c.png": Self.png]))
        guard case .draft(let draft) = await model.prepare("Example.com/a?utm_source=x#top") else {
            Issue.record("expected a draft")
            return
        }
        #expect(draft.address == "https://example.com/a")
        #expect(draft.details.title == "Hello" && draft.details.site == "The Site")
        #expect(draft.cover != nil && draft.failure == nil)

        let saved = try #require(model.save(draft, title: "Renamed"))
        #expect(model.bookmarks.map(\.id) == [saved.id])
        #expect(model.bookmarks.first?.title == "Renamed")
        #expect(model.cover(of: saved) != nil)
    }

    @Test func aFailedDownloadStillSavesTheAddressAloneWithTheReason() async throws {
        let model = try model(serving([:]))
        guard case .draft(let draft) = await model.prepare("https://offline.test/x") else {
            Issue.record("expected a draft")
            return
        }
        #expect(draft.failure != nil)
        #expect(draft.details.title == "https://offline.test/x")
        let saved = try #require(model.save(draft, title: ""))
        #expect(saved.title == "https://offline.test/x" && saved.cover == nil)
    }

    @Test func aPDFIsNotDownloaded() async throws {
        let model = try model { _ in throw URLError(.badURL) }
        guard case .draft(let draft) = await model.prepare("https://example.com/paper.pdf") else {
            Issue.record("expected a draft")
            return
        }
        #expect(draft.failure == nil)
    }

    @Test func anAddressSavedAlreadyGivesItsBookmarkWithoutDownloading() async throws {
        let model = try model(serving(["https://example.com/a": Self.page]))
        guard case .draft(let draft) = await model.prepare("https://example.com/a") else { return }
        let saved = try #require(model.save(draft, title: ""))
        let again = await model.prepare("https://EXAMPLE.com/a?utm_campaign=y")
        #expect(again == .existing(saved))
    }

    @Test func somethingThatIsNoWebAddressIsInvalid() async throws {
        let model = try model(serving([:]))
        #expect(await model.prepare("ftp://example.com/a") == .invalid)
        #expect(await model.prepare("  ") == .invalid)
    }

    @Test func listsNewestSavedFirstAndDeleteTakesTheBookmark() async throws {
        let model = try model(serving([:]))
        for name in ["a", "b"] {
            guard case .draft(let draft) = await model.prepare("https://example.com/\(name)") else { return }
            model.save(draft, title: name)
            try await Task.sleep(for: .milliseconds(1100))  // `saved` counts seconds
        }
        #expect(model.bookmarks.map(\.title) == ["b", "a"])
        model.delete(id: model.bookmarks[0].id)
        #expect(model.bookmarks.map(\.title) == ["a"])
    }
}
