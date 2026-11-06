import Testing

@testable import ExcalidrawNative

/// Excalidraw's colour palette as the properties panel offers it beyond the
/// top picks: the grid, the shades of a colour and the hex code input.
@Suite struct ColorPaletteTests {
    @Test func theStrokeGridIsExcalidrawsFifteenColoursInTheDarkShade() {
        let grid = ColorPalette.grid(for: .stroke)
        #expect(grid.count == 15)
        #expect(Array(grid.prefix(5)) == ["transparent", "#ffffff", "#343a40", "#1e1e1e", "#846358"])
        #expect(grid[5] == "#0c8599")  // cyan, shade 4
        #expect(grid[14] == "#e03131")  // red, shade 4
    }

    @Test func theBackgroundGridUsesTheLightShade() {
        let grid = ColorPalette.grid(for: .background)
        #expect(grid[2] == "#e9ecef")  // gray, shade 1
        #expect(grid[14] == "#ffc9c9")  // red, shade 1
        #expect(grid[0] == "transparent")
    }

    @Test func everyTopPickIsInTheStrokeOrBackgroundGrid() {
        for color in PropertiesPanel.strokeColors where color != "#1e1e1e" {
            #expect(ColorPalette.shades(of: color) != nil, "\(color)")
        }
        for color in PropertiesPanel.backgroundColors where color != "transparent" {
            #expect(ColorPalette.shades(of: color) != nil, "\(color)")
        }
    }

    @Test func aPaletteColourHasItsFiveShades() {
        #expect(ColorPalette.shades(of: "#e03131") == ["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"])
        #expect(ColorPalette.shades(of: "#FF8787")?.count == 5)
    }

    @Test func otherColoursHaveNoShades() {
        #expect(ColorPalette.shades(of: "#123456") == nil)
        #expect(ColorPalette.shades(of: "transparent") == nil)
        #expect(ColorPalette.shades(of: "#ffffff") == nil)
        #expect(ColorPalette.shades(of: nil) == nil)
    }

    @Test func hexInputIsNormalisedLikeExcalidraw() {
        #expect(ColorPalette.color(fromHex: "E03131") == "#e03131")
        #expect(ColorPalette.color(fromHex: " #E03131 ") == "#e03131")
        #expect(ColorPalette.color(fromHex: "f00") == "#f00")
        #expect(ColorPalette.color(fromHex: "f008") == "#f008")
        #expect(ColorPalette.color(fromHex: "ff000080") == "#ff000080")
    }

    @Test func invalidHexInputIsRejected() {
        #expect(ColorPalette.color(fromHex: "") == nil)
        #expect(ColorPalette.color(fromHex: "ff") == nil)
        #expect(ColorPalette.color(fromHex: "fffff") == nil)
        #expect(ColorPalette.color(fromHex: "gggggg") == nil)
        #expect(ColorPalette.color(fromHex: "#1234567") == nil)
    }

    @Test func everyPaletteColourHasASpokenName() {
        for kind in [ColorPalette.Kind.stroke, .background] {
            for color in ColorPalette.grid(for: kind).flatMap({ ColorPalette.shades(of: $0) ?? [$0] }) {
                #expect(!ColorPalette.name(color).key.hasPrefix("Colour"), "\(color)")
            }
        }
    }
}
