import Foundation

/// Excalidraw's colour palette (`COLOR_PALETTE`, open-color) as the colour
/// picker of the properties panel offers it: a grid of colours, the five
/// shades of a grid colour, and any other colour as a hex code.
enum ColorPalette {
    /// Which property a colour is picked for; it decides the grid's shade.
    enum Kind {
        case stroke, background

        /// Excalidraw's default shade per kind: dark for strokes, light for
        /// backgrounds.
        fileprivate var gridShade: Int { self == .stroke ? 4 : 1 }
    }

    /// A hue and its five shades, light to dark (open-color 50/200/400/600/800).
    private struct Hue {
        let name: LocalizedStringResource
        let shades: [String]
    }

    private static let hues: [Hue] = [
        Hue(name: "Gray", shades: ["#f8f9fa", "#e9ecef", "#ced4da", "#868e96", "#343a40"]),
        Hue(name: "Red", shades: ["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"]),
        Hue(name: "Pink", shades: ["#fff0f6", "#fcc2d7", "#f783ac", "#e64980", "#c2255c"]),
        Hue(name: "Grape", shades: ["#f8f0fc", "#eebefa", "#da77f2", "#be4bdb", "#9c36b5"]),
        Hue(name: "Violet", shades: ["#f3f0ff", "#d0bfff", "#9775fa", "#7950f2", "#6741d9"]),
        Hue(name: "Blue", shades: ["#e7f5ff", "#a5d8ff", "#4dabf7", "#228be6", "#1971c2"]),
        Hue(name: "Cyan", shades: ["#e3fafc", "#99e9f2", "#3bc9db", "#15aabf", "#0c8599"]),
        Hue(name: "Teal", shades: ["#e6fcf5", "#96f2d7", "#38d9a9", "#12b886", "#099268"]),
        Hue(name: "Green", shades: ["#ebfbee", "#b2f2bb", "#69db7c", "#40c057", "#2f9e44"]),
        Hue(name: "Yellow", shades: ["#fff9db", "#ffec99", "#ffd43b", "#fab005", "#f08c00"]),
        Hue(name: "Orange", shades: ["#fff4e6", "#ffd8a8", "#ffa94d", "#fd7e14", "#e8590c"]),
        Hue(name: "Bronze", shades: ["#f8f1ee", "#eaddd7", "#d2bab0", "#a18072", "#846358"]),
    ]

    /// The picker grid, row by row (5 × 3), in Excalidraw's order.
    static func grid(for kind: Kind) -> [String] {
        func shade(_ hue: Int) -> String { hues[hue].shades[kind.gridShade] }
        // gray, red, pink, grape, violet, blue, cyan, teal, green, yellow, orange, bronze
        return [
            "transparent", "#ffffff", shade(0), "#1e1e1e", shade(11),
            shade(6), shade(5), shade(4), shade(3), shade(2),
            shade(8), shade(7), shade(9), shade(10), shade(1),
        ]
    }

    /// The five shades of a palette colour, light to dark; `nil` for any
    /// other colour (black, white, transparent, a custom hex code).
    static func shades(of color: String?) -> [String]? {
        guard let color = color?.lowercased() else { return nil }
        return hues.first { $0.shades.contains(color) }?.shades
    }

    /// The colour a typed hex code stands for, `#` optional; `nil` if it
    /// is not 3, 4, 6 or 8 hex digits (Excalidraw's rule).
    static func color(fromHex input: String) -> String? {
        var digits = input.trimmingCharacters(in: .whitespaces).lowercased()
        if digits.hasPrefix("#") { digits.removeFirst() }
        guard [3, 4, 6, 8].contains(digits.count), digits.allSatisfy(\.isHexDigit) else { return nil }
        return "#" + digits
    }

    /// What VoiceOver reads for a colour: Excalidraw's names for the top
    /// picks, the hue and shade for other palette colours.
    static func name(_ color: String) -> LocalizedStringResource {
        switch color.lowercased() {
        case "transparent": return "Transparent"
        case "#1e1e1e": return "Black"
        case "#ffffff": return "White"
        case "#e03131": return "Red"
        case "#2f9e44": return "Green"
        case "#1971c2": return "Blue"
        case "#f08c00": return "Orange"
        case "#ffc9c9": return "Light red"
        case "#b2f2bb": return "Light green"
        case "#a5d8ff": return "Light blue"
        case "#ffec99": return "Light yellow"
        default: break
        }
        let color = color.lowercased()
        for hue in hues {
            if let shade = hue.shades.firstIndex(of: color) {
                return "\(String(localized: hue.name)), shade \(shade + 1)"
            }
        }
        return "Colour \(color)"
    }
}
