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
