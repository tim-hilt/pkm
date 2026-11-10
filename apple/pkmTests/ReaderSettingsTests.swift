import Foundation
import SwiftUI
import Testing

@testable import pkm

@MainActor @Suite struct ReaderSettingsTests {
    /// This Device's user defaults: one suite per test.
    let defaults = UserDefaults(suiteName: UUID().uuidString)!

    @Test func pageStyleDefaultsToLightPages() {
        #expect(ReaderSettings(defaults: defaults).pageStyle == .light)
    }

    @Test(arguments: [PageStyle.dark, .light, .match])
    func chosenPageStyleSurvivesARestart(style: PageStyle) {
        ReaderSettings(defaults: defaults).pageStyle = style
        #expect(ReaderSettings(defaults: defaults).pageStyle == style)
    }

    @Test func unknownSavedPageStyleFallsBackToLightPages() {
        defaults.set("sepia", forKey: "books.pageStyle")
        #expect(ReaderSettings(defaults: defaults).pageStyle == .light)
    }

    @Test func pagesAreDrawnPerPageStyle() {
        let settings = ReaderSettings(defaults: defaults)
        #expect(settings.pageAppearance(systemScheme: .light) == .light)
        settings.pageStyle = .dark
        #expect(settings.pageAppearance(systemScheme: .light) == .dark(.default))
    }

    @Test(arguments: [(ColorScheme.light, false), (.dark, true)])
    func matchAppearanceFollowsTheSchemeTheReaderShowsIn(scheme: ColorScheme, dark: Bool) {
        let settings = ReaderSettings(defaults: defaults)
        settings.pageStyle = .match
        #expect(settings.arePagesDark(systemScheme: scheme) == dark)
    }

    @Test(arguments: [ColorScheme.light, .dark])
    func lightAndDarkPagesIgnoreTheScheme(scheme: ColorScheme) {
        let settings = ReaderSettings(defaults: defaults)
        settings.pageStyle = .light
        #expect(!settings.arePagesDark(systemScheme: scheme))
        settings.pageStyle = .dark
        #expect(settings.arePagesDark(systemScheme: scheme))
    }

    @Test(arguments: [
        (PageStyle.light, ColorScheme.light, PageStyle.dark),
        (.dark, .light, .light),
        (.match, .light, .dark),
        (.match, .dark, .light),
    ])
    func shortcutSwitchesToTheOppositeOfWhatIsShowing(style: PageStyle, scheme: ColorScheme, target: PageStyle) {
        let settings = ReaderSettings(defaults: defaults)
        settings.pageStyle = style
        #expect(settings.oppositePageStyle(systemScheme: scheme) == target)
        settings.switchToOppositePageStyle(systemScheme: scheme)
        #expect(settings.pageStyle == target)
    }

    @Test func pageColorsDefaultToDarkGreyAndLightGrey() {
        #expect(ReaderSettings(defaults: defaults).pageColors == .default)
    }

    @Test func chosenPageColorsSurviveARestart() {
        ReaderSettings(defaults: defaults).pageColors = .sepia
        #expect(ReaderSettings(defaults: defaults).pageColors == .sepia)
    }

    @Test func resetRestoresTheDefaultPageColors() {
        let settings = ReaderSettings(defaults: defaults)
        settings.pageColors = .sepia
        settings.resetPageColors()
        #expect(settings.pageColors == .default)
        #expect(ReaderSettings(defaults: defaults).pageColors == .default)
    }

    @Test func unreadableSavedPageColorsFallBackToTheDefaults() {
        defaults.set([0.1, 0.2], forKey: "books.pageColors")
        #expect(ReaderSettings(defaults: defaults).pageColors == .default)
    }

    @Test func darkPagesAreDrawnInTheChosenPageColors() {
        let settings = ReaderSettings(defaults: defaults)
        settings.pageStyle = .dark
        settings.pageColors = .sepia
        #expect(settings.pageAppearance(systemScheme: .light) == .dark(.sepia))
    }

    @Test func epubTextSizeDefaultsToTheDesignedSize() {
        #expect(ReaderSettings(defaults: defaults).epubTextSize == 1)
    }

    @Test func epubTextSizeSurvivesARestart() {
        ReaderSettings(defaults: defaults).epubTextSize = 1.5
        #expect(ReaderSettings(defaults: defaults).epubTextSize == 1.5)
    }

    @Test func aSavedEPUBTextSizeOutOfRangeIsClamped() {
        defaults.set(40.0, forKey: "books.epubTextSize")
        #expect(ReaderSettings(defaults: defaults).epubTextSize == 3)
    }
}

extension PageColors {
    fileprivate static let sepia = PageColors(
        paper: RGB(0x2B / 255, 0x21 / 255, 0x16 / 255), ink: RGB(0xE0 / 255, 0xC8 / 255, 0xA0 / 255))
}
