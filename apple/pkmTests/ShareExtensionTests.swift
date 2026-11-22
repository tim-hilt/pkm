import Foundation
import Testing

@testable import pkm

/// The Share extension's side of Saving: a page handed over by a browser, and the hand-over of
/// what it Saved to the app.
@MainActor @Suite struct ShareExtensionTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: UUID().uuidString, directoryHint: .isDirectory)

    private static let article = """
        <title>Behind a login</title><article><p>\(String(
            repeating: "Plenty of prose, indeed, and a <a href=\"/x\">link</a>. ", count: 20))</p></article>
        """

    /// A model that fails every download: the Share sheet had the page, and there is no connection.
    private func offlineModel(_ folder: String) throws -> BookmarksModel {
        BookmarksModel(
            store: try BookmarkStore.open(root: root.appending(path: folder).path),
            download: { _ in throw URLError(.notConnectedToInternet) })
    }

    @Test func aPageFromTheBrowserIsSavedWithItsArticleWithoutDownloading() async throws {
        let model = try offlineModel("Bookmarks")
        guard case .draft(let draft) = await model.prepare("https://example.com/a", html: Self.article) else {
            Issue.record("expected a draft")
            return
        }
        #expect(draft.failure == nil && draft.article.readsLikeArticle)
        #expect(draft.details.title == "Behind a login")
        let saved = try #require(model.save(draft, title: "", keepsArticle: true))
        #expect(saved.hasArticle && model.article(of: saved)?.contains("[link](https://example.com/x)") == true)
    }

    @Test func aPageBeyondTheLimitIsKeptWithItsAddressOnly() async throws {
        let model = try offlineModel("Bookmarks")
        let huge = String(repeating: "x", count: Download.limit + 1)
        guard case .draft(let draft) = await model.prepare("https://example.com/a", html: huge) else { return }
        #expect(draft.failure != nil && draft.article.markdown.isEmpty)
    }

    @Test func anAddressSavedInTheAppIsFoundAndOffersAddArticle() async throws {
        let model = try offlineModel("Inbox")
        let plain = ShareHandoff.Entry(address: "https://example.com/a", title: "Plain", hasArticle: false)
        model.savedElsewhere = [plain.bookmark]
        let again = await model.prepare("https://example.com/a?utm_source=x", html: Self.article)
        guard case .existing(let found) = again else {
            Issue.record("expected the Bookmark Saved in the app")
            return
        }
        #expect(found.title == "Plain" && !found.hasArticle)
        #expect(await model.addArticle(to: found, html: Self.article))
        #expect(model.bookmarks.map(\.address) == ["https://example.com/a"])
        #expect(model.bookmarks.first?.hasArticle == true)
    }

    @Test func whatTheExtensionSavedReachesTheAppAndItsArticleJoinsAPlainBookmark() async throws {
        let handoff = ShareHandoff(folder: root.appending(path: "Group"))
        let inbox = BookmarksModel(store: try handoff.openInbox(), download: { _ in throw URLError(.badURL) })
        let app = try offlineModel("Bookmarks")
        app.handoff = handoff
        guard case .draft(let plain) = await app.prepare("https://example.com/plain", html: "<title>Plain</title>"),
            case .draft(let fresh) = await inbox.prepare("https://example.com/new", html: Self.article),
            case .draft(let article) = await inbox.prepare("https://example.com/plain", html: Self.article)
        else { return }
        let kept = try #require(app.save(plain, title: "", tags: ["mine"], keepsArticle: false))
        inbox.save(fresh, title: "Shared", tags: ["rust"], keepsArticle: true)
        inbox.save(article, title: "", keepsArticle: true)

        app.exchangeWithShareExtension(tags: ["mine"])

        #expect(Set(app.bookmarks.map(\.address)) == ["https://example.com/new", "https://example.com/plain"])
        let new = try #require(app.bookmarks.first { $0.address == "https://example.com/new" })
        #expect(new.title == "Shared" && new.tags == ["rust"] && new.hasArticle)
        #expect(app.bookmarks.first { $0.id == kept.id }?.hasArticle == true)
        #expect(try handoff.openInbox().bookmarks().isEmpty)
        let published = handoff.published()
        #expect(published.tags == ["mine"] && published.bookmarks.count == 2)
        #expect(
            published.bookmarks.contains(.init(address: "https://example.com/new", title: "Shared", hasArticle: true)))
    }

    @Test func theBuiltGroupIsUsedWhereTheSignatureGrantsIt() {
        let group = ShareHandoff.group(listed: ["group.com.timhilt.pkm.TEAM"]) { _ in true }
        #expect(group == ShareHandoff.group)
    }

    @Test func aSideloadedBuildUsesTheGroupRenamedForItsTeam() {
        let renamed = "group.com.timhilt.pkm.TEAM"
        let listed = ["group.com.other", "group.com.timhilt.pkmother", renamed]
        #expect(ShareHandoff.group(listed: listed) { $0 != ShareHandoff.group } == renamed)
        #expect(ShareHandoff.group(listed: listed) { _ in false } == nil)
    }

    @Test func publishedStartsEmptyBeforeTheAppHasPublished() {
        #expect(ShareHandoff(folder: root).published() == .init())
    }

    @Test func theBrowsersResultsGiveTheAddressAndHTML() {
        let page = SharedPage(preprocessed: ["url": "https://example.com/a", "html": "<p>Hi</p>"])
        #expect(page == SharedPage(address: "https://example.com/a", html: "<p>Hi</p>"))
        #expect(SharedPage(preprocessed: ["html": "<p>Hi</p>"]) == nil)
    }

    @Test func htmlBeyondTheLimitIsLeftOutSoThePageIsDownloadedInstead() {
        let huge = String(repeating: "x", count: Download.limit + 1)
        #expect(SharedPage(preprocessed: ["url": "https://example.com/a", "html": huge])?.html == nil)
    }
}
