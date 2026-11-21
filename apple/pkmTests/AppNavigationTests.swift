import Foundation
import Testing

@testable import pkm

@MainActor @Suite struct AppNavigationTests {
    /// A Device's defaults, empty until the test fills them.
    private func device() -> UserDefaults {
        let name = "AppNavigationTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    @Test func theContextsComeInTheFixedOrderWithTheirSymbols() {
        #expect(AppContext.allCases.map(\.rawValue) == ["tasks", "notes", "drawings", "bookmarks", "books"])
        #expect(
            AppContext.allCases.map(\.symbol) == [
                "checklist", "note.text", "scribble.variable", "bookmark", "books.vertical",
            ])
    }

    @Test func theFirstLaunchOpensOnTasks() {
        #expect(AppNavigation(remembering: device()).selected == .tasks)
    }

    @Test func switchingSelectsTheContext() {
        let navigation = AppNavigation(remembering: device())
        navigation.selected = .books
        #expect(navigation.selected == .books)
    }

    @Test func theLastContextIsRestoredOnTheSameDevice() {
        let defaults = device()
        AppNavigation(remembering: defaults).selected = .bookmarks
        #expect(AppNavigation(remembering: defaults).selected == .bookmarks)
    }

    @Test func aDeviceThatRemembersNothingStartsOnTasks() {
        AppNavigation(remembering: nil).selected = .notes
        #expect(AppNavigation(remembering: nil).selected == .tasks)
    }

    @Test func anUnknownRememberedContextFallsBackToTasks() {
        let defaults = device()
        defaults.set("recipes", forKey: "lastContext")
        #expect(AppNavigation(remembering: defaults).selected == .tasks)
    }

    @Test func shortcutsAreCommandOneToFiveInOrder() {
        #expect(AppContext.allCases.map(\.shortcutDigit) == [1, 2, 3, 4, 5])
    }

    @Test func switchingIsAvailableUnlessAnItemIsOpen() {
        let navigation = AppNavigation(remembering: nil)
        #expect(navigation.canSwitchContext)
        navigation.isItemOpen = true
        #expect(!navigation.canSwitchContext)
        navigation.isItemOpen = false
        #expect(navigation.canSwitchContext)
    }

    @Test func theSettingsDestinationReplacesTheContextUntilOneIsSelected() {
        let navigation = AppNavigation(remembering: nil)
        navigation.selected = .books
        navigation.isShowingSettings = true
        #expect(navigation.destination == .settings)
        navigation.destination = .context(.drawings)
        #expect(navigation.destination == .context(.drawings))
        #expect(!navigation.isShowingSettings)
    }

    @Test func settingsAreNotRememberedAsTheLastContext() {
        let defaults = device()
        let navigation = AppNavigation(remembering: defaults)
        navigation.selected = .notes
        navigation.destination = .settings
        #expect(AppNavigation(remembering: defaults).selected == .notes)
    }

    @Test func aLinkLeavesSettings() {
        let navigation = AppNavigation(remembering: nil)
        navigation.destination = .settings
        _ = navigation.follow(URL(string: "pkm://tasks")!)
        #expect(navigation.destination == .context(.tasks))
    }

    @Test func aContextLinkSwitchesToTheContext() {
        let navigation = AppNavigation(remembering: nil)
        #expect(navigation.follow(URL(string: "pkm://books")!) == .init(context: .books, itemID: nil))
        #expect(navigation.selected == .books)
    }

    @Test func anItemLinkSwitchesToTheContextAndNamesTheItem() {
        let navigation = AppNavigation(remembering: nil)
        #expect(navigation.follow(URL(string: "pkm://drawings/abc123")!) == .init(context: .drawings, itemID: "abc123"))
        #expect(navigation.selected == .drawings)
    }

    @Test func unknownContextsAndOtherURLsAreIgnored() {
        let navigation = AppNavigation(remembering: nil)
        #expect(navigation.follow(URL(string: "pkm://recipes/1")!) == nil)
        #expect(navigation.follow(URL(string: "https://drawings/1")!) == nil)
        #expect(navigation.selected == .tasks)
    }
}

// MARK: Links

extension AppNavigationTests {
    /// A window's link handling over a temporary Drawings folder and Library.
    private func makeLinks() throws -> (
        links: AppLinks, gallery: Gallery, library: LibraryModel, tasks: TasksModel, bookmarks: BookmarksModel
    ) {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "AppNavigationTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        let drawings = root.appending(path: "Drawings", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: drawings, withIntermediateDirectories: true)
        let store = DrawingStore(directory: drawings, cacheDirectory: root.appending(path: "Caches"))
        try store.save(Editor().save(), as: "sample")
        let gallery = Gallery(store: store)
        let library = LibraryModel(
            core: try Library.open(root: root.appending(path: "Books").path),
            coversFolder: root.appending(path: "Covers"),
            defaults: UserDefaults(suiteName: root.lastPathComponent)!)
        let tasks = TasksModel(
            store: try TaskStore.withFavorites(
                root: root.appending(path: "Tasks").path))
        let bookmarks = BookmarksModel(store: try BookmarkStore.open(root: root.appending(path: "Bookmarks").path))
        let links = AppLinks(
            navigation: AppNavigation(remembering: device()), gallery: gallery, library: library, tasks: tasks,
            bookmarks: bookmarks)
        return (links, gallery, library, tasks, bookmarks)
    }

    @Test func aDrawingLinkOpensTheDrawingAndAnUnknownOneShowsTheGallery() throws {
        let (links, gallery, _, _, _) = try makeLinks()
        let id = try #require(gallery.drawings.first?.id)

        links.follow(URL(string: "pkm://drawings/\(id)")!)
        #expect(links.navigation.selected == .drawings)
        #expect(gallery.open != nil)

        links.follow(URL(string: "pkm://drawings/unknown")!)
        #expect(links.navigation.selected == .drawings)
        #expect(gallery.open == nil)
    }

    @Test func aBookLinkOpensTheBookAndAnUnknownOneShowsTheGallery() async throws {
        let (links, gallery, library, _, _) = try makeLinks()
        guard case .imported(let book) = await library.import(try writePDF(named: "book.pdf")) else {
            Issue.record("expected the Book to be imported")
            return
        }

        links.follow(URL(string: "pkm://books/\(book.id)")!)
        #expect(links.navigation.selected == .books)
        #expect(library.open == book)

        links.follow(URL(string: "pkm://books/unknown")!)
        #expect(links.navigation.selected == .books)
        #expect(library.open == nil)
        #expect(gallery.open == nil)
    }

    @Test func aTaskLinkOpensTheTaskOverTheFirstFavoriteShowingItAndAnUnknownOneShowsTheStartList() throws {
        let (links, _, _, tasks, _) = try makeLinks()
        tasks.add(title: "Later", tags: ["backlog"])
        let id = try #require(tasks.tasks.first?.id)
        let start = tasks.list
        tasks.addFilter(title: "Backlog only", query: "@backlog")
        tasks.setFavorite(tasks.filters.last!.id, to: true)
        tasks.list = .filters

        links.follow(URL(string: "pkm://tasks/\(id)")!)
        #expect(links.navigation.selected == .tasks)
        #expect(tasks.list == start)
        #expect(tasks.editing == [id])

        links.follow(URL(string: "pkm://tasks/unknown")!)
        #expect(tasks.list == start)
        #expect(tasks.editing.isEmpty)
    }

    @Test func aContextLinkToTasksClosesAnOpenTask() throws {
        let (links, _, _, tasks, _) = try makeLinks()
        tasks.add(title: "Now")
        links.follow(URL(string: "pkm://tasks/\(tasks.tasks[0].id)")!)
        links.follow(URL(string: "pkm://tasks")!)
        #expect(tasks.editing.isEmpty)
    }

    @Test func aLinkToAClosedSubtaskOpensNoSheet() throws {
        let (links, _, _, tasks, _) = try makeLinks()
        tasks.add(title: "Parent")
        _ = tasks.addSubtask(title: "Sub", under: tasks.tasks[0])
        let sub = tasks.tasks[1]
        tasks.toggleDone(sub)
        tasks.commit()

        links.follow(URL(string: "pkm://tasks/\(sub.id)")!)
        #expect(tasks.editing.isEmpty)
    }

    @Test func aBookmarkLinkOpensItsDetailsAndAMissingOneLeavesTheListShown() async throws {
        let (links, _, _, _, bookmarks) = try makeLinks()
        let draft = SaveDraft(
            address: "https://example.com/a",
            details: Details(
                title: "A", site: "", cover: nil, excerpt: "", author: "", published: ""), cover: nil, images: [:],
            article: Article(markdown: "", readsLikeArticle: false, images: []), failure: nil)
        let id = try #require(bookmarks.save(draft, title: "", keepsArticle: false)).id

        links.follow(URL(string: "pkm://bookmarks/\(id)")!)
        #expect(links.navigation.selected == .bookmarks)
        #expect(bookmarks.detailsID == id)

        links.follow(URL(string: "pkm://bookmarks/missing")!)
        #expect(links.navigation.selected == .bookmarks)
        #expect(bookmarks.detailsID == nil && bookmarks.openID == nil)
    }
}
