import SwiftUI

extension TagEntry {
    /// The Tag Color as picked, used as it is in Light and Dark mode; `fallback`
    /// for Default, which adapts to Dark mode.
    func style(default fallback: some ShapeStyle = .secondary) -> AnyShapeStyle {
        color.flatMap(Color.init(tagColor:)).map { AnyShapeStyle($0) } ?? AnyShapeStyle(fallback)
    }
}

extension Color {
    /// From `#rrggbb` (sRGB); nil if malformed.
    init?(tagColor hex: String) {
        guard hex.count == 7, hex.hasPrefix("#"), let rgb = UInt32(hex.dropFirst(), radix: 16) else { return nil }
        self.init(
            .sRGB, red: Double(rgb >> 16 & 0xFF) / 255, green: Double(rgb >> 8 & 0xFF) / 255,
            blue: Double(rgb & 0xFF) / 255)
    }

    /// As `#rrggbb` (sRGB), clamped to the gamut.
    var tagColor: String {
        let c = resolve(in: EnvironmentValues())
        let byte = { (v: Float) in Int((min(max(v, 0), 1) * 255).rounded()) }
        return String(format: "#%02x%02x%02x", byte(c.red), byte(c.green), byte(c.blue))
    }
}
