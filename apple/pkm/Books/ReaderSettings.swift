import SwiftUI

/// How the Reader draws Books, kept on this Device in user defaults (never
/// synced) and shared by every Reader: Page Style, Page Colors and the text
/// size of EPUBs. Changing one redraws open Readers at once and is saved.
@MainActor @Observable
final class ReaderSettings {
    var pageStyle: PageStyle {
        didSet { defaults.set(pageStyle.rawValue, forKey: Self.pageStyleName) }
    }

    /// The paper and ink of Dark Pages.
    var pageColors: PageColors {
        didSet { defaults.set(pageColors.components, forKey: Self.pageColorsName) }
    }

    /// The text size of all EPUBs (`TextSize`).
    var epubTextSize: CGFloat {
        didSet { defaults.set(Double(epubTextSize), forKey: Self.epubTextSizeName) }
    }

    func resetPageColors() { pageColors = .default }

    /// Whether pages are dark under the current Page Style. "Match Appearance"
    /// follows `systemScheme`, the color scheme the Reader shows in, which is
    /// the app's Appearance.
    func arePagesDark(systemScheme: ColorScheme) -> Bool {
        switch pageStyle {
        case .light: false
        case .dark: true
        case .match: systemScheme == .dark
        }
    }

    /// The Page Style ⇧⌘D switches to: Dark Pages if pages look light, else Light Pages.
    func oppositePageStyle(systemScheme: ColorScheme) -> PageStyle {
        arePagesDark(systemScheme: systemScheme) ? .light : .dark
    }

    func switchToOppositePageStyle(systemScheme: ColorScheme) {
        pageStyle = oppositePageStyle(systemScheme: systemScheme)
    }

    /// How pages are drawn under the current Page Style.
    func pageAppearance(systemScheme: ColorScheme) -> PageAppearance {
        arePagesDark(systemScheme: systemScheme) ? .dark(pageColors) : .light
    }

    @ObservationIgnored private let defaults: UserDefaults
    private static let pageStyleName = "books.pageStyle"
    private static let pageColorsName = "books.pageColors"
    private static let epubTextSizeName = "books.epubTextSize"

    init(defaults: UserDefaults) {
        self.defaults = defaults
        pageStyle = defaults.string(forKey: Self.pageStyleName).flatMap(PageStyle.init) ?? .light
        pageColors = (defaults.array(forKey: Self.pageColorsName) as? [Double]).flatMap(PageColors.init) ?? .default
        epubTextSize =
            defaults.object(forKey: Self.epubTextSizeName) is Double
            ? TextSize.clamped(defaults.double(forKey: Self.epubTextSizeName)) : TextSize.normal
    }

    /// The settings for this launch: the standard defaults, or a temporary
    /// suite for UI tests and screenshots (`-UITesting…`).
    static func forLaunch() -> ReaderSettings {
        let isolated = ProcessInfo.processInfo.arguments.contains { $0.hasPrefix("-UITesting") }
        return ReaderSettings(defaults: isolated ? UserDefaults(suiteName: UUID().uuidString)! : .standard)
    }
}
