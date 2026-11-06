import SwiftUI

#if canImport(UIKit)
    import UIKit
    private typealias ChromeNativeColor = UIColor
#else
    import AppKit
    private typealias ChromeNativeColor = NSColor
#endif

/// The chrome's hand-picked colours: Excalidraw's light values and, from its
/// dark theme (`theme.scss`), the values for Dark mode. The system picks the
/// variant, so a change of appearance needs no code.
enum Chrome {
    /// Island and sheet surface: `--island-bg-color`.
    static let island = adaptive(light: 0xFFFFFF, dark: 0x232329)
    /// Grey control group fill: `--color-surface-low`.
    static let controlFill = adaptive(light: 0xECECF4, dark: 0x232329)
    /// Checked tool fill: `--color-surface-primary-container`.
    static let checkedFill = adaptive(light: 0xE0DFFF, dark: 0x403E6A)
    /// Checked tool glyph: `--color-on-primary-container`.
    static let checkedGlyph = adaptive(light: 0x030064, dark: 0xE0DFFF)
    /// Divider between tool groups.
    static let divider = adaptive(light: 0xF1F0FF, dark: 0x46464F)
    /// Colour picker panel: `--color-surface-mid`.
    static let pickerPanel = adaptive(light: 0xF7F7F7, dark: 0x18181B)
    /// Text field fill and border: `--input-bg-color`, `--input-border-color`.
    static let inputFill = adaptive(light: 0xFFFFFF, dark: 0x121212)
    static let inputBorder = adaptive(light: 0xEBEBEB, dark: 0x2E2E2E)
    /// Selected panel option: Excalidraw's accent (`--color-primary-light`).
    static let optionSelected = adaptive(light: 0xE3E2FE, dark: 0x4F4D6F)
    /// Checkerboard behind transparent swatches.
    static let checkerboard = adaptive(light: 0xFFFFFF, dark: 0x232329)

    /// The colour as the given appearance resolves it, as RGB bytes.
    static func rgb(_ color: Color, dark: Bool) -> [Int] {
        let traits = ChromeNativeColor(color)
        #if canImport(UIKit)
            let resolved = traits.resolvedColor(
                with: UITraitCollection(userInterfaceStyle: dark ? .dark : .light))
            var (r, g, b, a) = (CGFloat(0), CGFloat(0), CGFloat(0), CGFloat(0))
            resolved.getRed(&r, green: &g, blue: &b, alpha: &a)
        #else
            var (r, g, b) = (CGFloat(0), CGFloat(0), CGFloat(0))
            NSAppearance(named: dark ? .darkAqua : .aqua)!.performAsCurrentDrawingAppearance {
                let c = traits.usingColorSpace(.sRGB)!
                r = c.redComponent
                g = c.greenComponent
                b = c.blueComponent
            }
        #endif
        return [r, g, b].map { Int(($0 * 255).rounded()) }
    }

    private static func adaptive(light: UInt32, dark: UInt32) -> Color {
        #if canImport(UIKit)
            Color(uiColor: UIColor { $0.userInterfaceStyle == .dark ? UIColor(hex: dark) : UIColor(hex: light) })
        #else
            Color(
                nsColor: NSColor(name: nil) {
                    $0.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? NSColor(hex: dark) : NSColor(hex: light)
                })
        #endif
    }
}

extension ChromeNativeColor {
    fileprivate convenience init(hex: UInt32) {
        self.init(
            red: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
    }
}
