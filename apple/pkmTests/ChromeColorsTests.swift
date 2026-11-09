import SwiftUI
import Testing

@testable import pkm

#if canImport(UIKit)
    import UIKit
#else
    import AppKit
#endif

/// The chrome's colours in the two appearances: Excalidraw's light values
/// stay as they were, Dark mode uses its dark theme's.
@Suite struct ChromeColorsTests {
    @Test func theIslandIsWhiteInLightAndExcalidrawsDarkGreyInDark() {
        #expect(rgb(Chrome.island, dark: false) == [0xFF, 0xFF, 0xFF])
        #expect(rgb(Chrome.island, dark: true) == [0x23, 0x23, 0x29])
    }

    @Test func checkedControlsKeepTheirLightColoursAndUseTheDarkAccentInDark() {
        #expect(rgb(Chrome.checkedFill, dark: false) == [0xE0, 0xDF, 0xFF])
        #expect(rgb(Chrome.checkedGlyph, dark: false) == [0x03, 0x00, 0x64])
        #expect(rgb(Chrome.checkedFill, dark: true) == [0x40, 0x3E, 0x6A])
        #expect(rgb(Chrome.checkedGlyph, dark: true) == [0xE0, 0xDF, 0xFF])
    }

    @Test func theAccentIsExcalidrawsPurpleInBothAppearances() {
        #expect(rgb(Color.accentColor, dark: false) == [0x69, 0x65, 0xDB])
        #expect(rgb(Color.accentColor, dark: true) == [0xA8, 0xA5, 0xFF])
    }

    /// `color` as the given appearance resolves it, as RGB bytes.
    private func rgb(_ color: Color, dark: Bool) -> [Int] {
        #if canImport(UIKit)
            let resolved = UIColor(color).resolvedColor(
                with: UITraitCollection(userInterfaceStyle: dark ? .dark : .light))
            var (r, g, b, a) = (CGFloat(0), CGFloat(0), CGFloat(0), CGFloat(0))
            resolved.getRed(&r, green: &g, blue: &b, alpha: &a)
        #else
            var (r, g, b) = (CGFloat(0), CGFloat(0), CGFloat(0))
            NSAppearance(named: dark ? .darkAqua : .aqua)!.performAsCurrentDrawingAppearance {
                let c = NSColor(color).usingColorSpace(.sRGB)!
                r = c.redComponent
                g = c.greenComponent
                b = c.blueComponent
            }
        #endif
        return [r, g, b].map { Int(($0 * 255).rounded()) }
    }
}
