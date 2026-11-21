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
        #expect(draft.images.keys.sorted() == ["https://example.com/c.png"] && draft.failure == nil)

        let saved = try #require(model.save(draft, title: "Renamed", keepsArticle: false))
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
        let saved = try #require(model.save(draft, title: "", keepsArticle: false))
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
        let saved = try #require(model.save(draft, title: "", keepsArticle: false))
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
            model.save(draft, title: name, keepsArticle: false)
            try await Task.sleep(for: .milliseconds(1100))  // `saved` counts seconds
        }
        #expect(model.bookmarks.map(\.title) == ["b", "a"])
        model.delete(id: model.bookmarks[0].id)
        #expect(model.bookmarks.map(\.title) == ["a"])
    }

    private static let longPage = Download(
        data: Data(
            ("<title>Long</title><article><p>"
                + String(repeating: "Plenty of prose, indeed, and a <a href=\"/x\">link</a>. ", count: 20)
                + "</p></article>").utf8),
        mimeType: "text/html", textEncoding: "utf-8")

    private static let shortPage = Download(
        data: Data("<title>Home</title><p>Welcome, friends, come in.</p>".utf8),
        mimeType: "text/html", textEncoding: "utf-8")

    @Test func aPageThatReadsLikeAnArticleIsSavedWithItsTextAndRead() async throws {
        let model = try model(serving(["https://example.com/long": Self.longPage]))
        guard case .draft(let draft) = await model.prepare("https://example.com/long") else {
            Issue.record("expected a draft")
            return
        }
        #expect(draft.article.readsLikeArticle)
        let saved = try #require(model.save(draft, title: "", keepsArticle: draft.article.readsLikeArticle))
        #expect(saved.hasArticle)
        let text = try #require(model.article(of: saved))
        #expect(text.contains("[link](https://example.com/x)"))
    }

    @Test func theUsersToggleOverridesTheDecision() async throws {
        let model = try model(
            serving([
                "https://example.com/long": Self.longPage, "https://example.com/a": Self.shortPage,
            ]))
        guard case .draft(let long) = await model.prepare("https://example.com/long"),
            case .draft(let short) = await model.prepare("https://example.com/a")
        else { return }
        #expect(!short.article.readsLikeArticle)
        let without = try #require(model.save(long, title: "", keepsArticle: false))
        let with = try #require(model.save(short, title: "", keepsArticle: true))
        #expect(!without.hasArticle && model.article(of: without) == nil)
        #expect(with.hasArticle)
    }

    @Test func deletingTheBookmarkClosesItsReader() async throws {
        let model = try model(serving(["https://example.com/long": Self.longPage]))
        guard case .draft(let draft) = await model.prepare("https://example.com/long") else { return }
        let saved = try #require(model.save(draft, title: "", keepsArticle: true))
        model.openID = saved.id
        #expect(model.pushedPath == [saved.id])
        model.delete(id: saved.id)
        #expect(model.pushedPath.isEmpty)
    }

    private static let pictures = Download(
        data: Data(
            ("<title>Pics</title><article><p>"
                + String(repeating: "Plenty of prose, indeed, and more of it. ", count: 20)
                + "</p><img src=\"/ok.png\" alt=\"Fine\"><img src=\"/missing.png\">"
                + "<img srcset=\"/small.png 1x, /ok.png 2x\"></article>").utf8),
        mimeType: "text/html", textEncoding: "utf-8")

    @Test func anArticleKeepsItsImagesAndDropsTheOnesThatFailed() async throws {
        let model = try model(
            serving(["https://example.com/pics": Self.pictures, "https://example.com/ok.png": Self.png]))
        var steps: [Double] = []
        guard case .draft(let draft) = await model.prepare("https://example.com/pics", progress: { steps.append($0) })
        else {
            Issue.record("expected a draft")
            return
        }
        #expect(draft.images.keys.sorted() == ["https://example.com/ok.png"])
        #expect(steps.last == 1 && steps == steps.sorted())

        let saved = try #require(model.save(draft, title: "", keepsArticle: true))
        let text = try #require(model.article(of: saved))
        #expect(!text.contains("http") && !text.contains("missing"))
        let names = text.split(separator: "(").dropFirst().map { $0.prefix { $0 != ")" } }
        #expect(names.count == 2 && names[0] == names[1])
        #expect(model.image(String(names[0]), of: saved.id) == draft.images["https://example.com/ok.png"])
    }

    /// A model with the Bookmarks of `addresses` Saved, newest last, without Article.
    private func saved(_ addresses: [String], pages: [String: Download] = [:]) async throws -> BookmarksModel {
        let model = try model(serving(pages))
        for address in addresses {
            guard case .draft(let draft) = await model.prepare(address) else { continue }
            model.save(draft, title: address, keepsArticle: false)
        }
        return model
    }

    @Test func aBookmarkIsRenamedAndTaggedAndTheTagsAreSetAsAWhole() async throws {
        let model = try await saved(["https://example.com/a"])
        let id = model.bookmarks[0].id
        model.rename(id: id, to: "Mine")
        model.setTags(["work", "home"], of: id)
        #expect(model.bookmarks[0].title == "Mine" && model.bookmarks[0].tags == ["work", "home"])
        model.setTags(["home", "new"], of: id)
        #expect(model.bookmarks[0].tags == ["home", "new"])
    }

    @Test func theListIsNarrowedToATagAndBackAndADeletedTagLeavesTheNarrowing() async throws {
        let model = try await saved(["https://example.com/a", "https://example.com/b"])
        let (a, b) = (model.bookmarks[0].id, model.bookmarks[1].id)
        model.setTags(["work"], of: a)
        model.setTags(["home"], of: b)
        #expect(model.tagsInUse == ["home", "work"])
        model.tagFilter = "work"
        #expect(model.shown.map(\.id) == [a])
        model.removeTagEverywhere("work")
        #expect(model.tagFilter == nil && model.shown.count == 2 && model.bookmarks.allSatisfy { $0.tags != ["work"] })
        #expect(model.tagsInUse == ["home"])
    }

    @Test func theSaveSheetsTagsAreKept() async throws {
        let model = try model(serving(["https://example.com/a": Self.page]))
        guard case .draft(let draft) = await model.prepare("https://example.com/a") else { return }
        let saved = try #require(model.save(draft, title: "", tags: ["read-later"], keepsArticle: false))
        #expect(saved.tags == ["read-later"] && model.bookmarks[0].tags == ["read-later"])
    }

    @Test func anArticleIsAddedFromThePageAsItIsNowAndDroppedAgain() async throws {
        let model = try await saved(["https://example.com/long"], pages: ["https://example.com/long": Self.shortPage])
        let id = model.bookmarks[0].id
        #expect(!model.bookmarks[0].hasArticle)
        let added = await model.addArticle(to: model.bookmarks[0])
        #expect(added && model.bookmarks[0].hasArticle)
        #expect(model.article(of: model.bookmarks[0])?.contains("Welcome") == true)
        model.dropArticle(id: id)
        #expect(!model.bookmarks[0].hasArticle && model.bookmarks[0].title == "https://example.com/long")
    }

    @Test func noArticleIsAddedWhenThePageCantBeDownloaded() async throws {
        let model = try await saved(["https://example.com/a"], pages: ["https://example.com/a": Self.shortPage])
        let offline = try self.model(serving([:]))
        #expect(await offline.addArticle(to: offline.bookmarks[0]) == false)
        #expect(!offline.bookmarks[0].hasArticle)
    }

    @Test func aLinkShowsTheDetailsOfABookmarkAndAMissingOneShowsNone() async throws {
        let model = try await saved(["https://example.com/a"])
        let id = model.bookmarks[0].id
        model.openID = id
        model.openLinked(id: id)
        #expect(model.detailsID == id && model.openID == nil)
        model.openLinked(id: "missing")
        #expect(model.detailsID == nil && model.openID == nil)
    }

    @Test func markupIsPutOnAnArticleRecoloredCommentedAndRemoved() async throws {
        let model = try await saved(["https://example.com/long"], pages: ["https://example.com/long": Self.shortPage])
        let id = model.bookmarks[0].id
        _ = await model.addArticle(to: model.bookmarks[0])
        let markdown = try #require(model.article(of: model.bookmarks[0]))
        let word = (markdown as NSString).range(of: "Welcome")
        let yellow = Rgb(r: 255, g: 235, b: 60)
        let green = Rgb(r: 126, g: 217, b: 87)

        let highlight = try #require(model.highlight(word, of: markdown, in: id, color: yellow))
        model.setColor(green, ofHighlight: highlight.id, in: id)
        model.setComment("why", ofHighlight: highlight.id, in: id)
        let spot = try #require(model.commentSpot(at: word.upperBound, in: id))
        model.setText("hm", ofComment: spot.id, in: id)

        let markup = model.markup(of: id)
        #expect(markup.highlights.map(\.quotedText) == ["Welcome"] && markup.highlights[0].color == green)
        #expect(markup.highlights[0].comment == "why" && markup.pageComments.map(\.text) == ["hm"])
        #expect(markup.target(at: word.location, length: markdown.utf16.count) == .highlight(highlight.id))
        #expect(markup.target(at: word.upperBound, length: markdown.utf16.count) == .comment(spot.id))
        #expect(markup.target(at: word.upperBound + 1, length: markdown.utf16.count) == nil)

        model.removeHighlight(highlight.id, in: id)
        model.removeComment(spot.id, in: id)
        #expect(model.markup(of: id).highlights.isEmpty && model.markup(of: id).pageComments.isEmpty)
        #expect(model.highlight(NSRange(location: 2, length: 0), of: markdown, in: id, color: yellow) == nil)
    }

    @Test func searchListsMatchesWithASnippetAndArticleSearchGivesRangesInTheArticle() async throws {
        let model = try await saved(["https://example.com/long"], pages: ["https://example.com/long": Self.shortPage])
        _ = await model.addArticle(to: model.bookmarks[0])
        let id = model.bookmarks[0].id
        let markdown = try #require(model.article(of: model.bookmarks[0]))

        model.searchText = "WELCOME"
        #expect(model.results.map(\.bookmark.id) == [id])
        let found = try #require(model.results.first)
        let units = Array(found.snippet.utf16)[Int(found.matchedStart)..<Int(found.matchedEnd)]
        #expect(String(decoding: units, as: UTF16.self).lowercased() == "welcome")
        model.searchText = ""
        #expect(model.results.isEmpty)

        let ranges = model.searchArticle("welcome", in: id)
        #expect(!ranges.isEmpty)
        #expect(ranges.allSatisfy { (markdown as NSString).substring(with: $0).lowercased() == "welcome" })
        #expect(model.searchArticle("  ", in: id).isEmpty)
    }
}
