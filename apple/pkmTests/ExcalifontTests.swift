import Foundation
import Testing

@testable import pkm

/// The `TextMeasurer` the core gets and the glyph outlines the renderer draws,
/// both from the bundled Excalifont. Expected numbers are read from the font
/// file itself (hmtx and glyf tables), not computed the way the code does.
@Suite struct ExcalifontTests {
    private func font() throws -> Excalifont { try #require(Excalifont.shared) }

    // MARK: Measuring

    @Test func theFontIsBundled() {
        #expect(Excalifont.shared != nil)
    }

    @Test func aGlyphIsAsWideAsTheFontSaysItsAdvanceIs() throws {
        let font = try font()

        // Advances in the font's 1000 units per em: space 400, "l" 225.
        #expect(font.lineWidth(text: " ", fontSize: 1000) == 400)
        #expect(font.lineWidth(text: "l", fontSize: 1000) == 225)
        #expect(abs(font.lineWidth(text: "l", fontSize: 20) - 4.5) < 1e-6)
    }

    @Test func widthGrowsWithTheLengthAndTheSizeOfTheText() throws {
        let font = try font()

        #expect(font.lineWidth(text: "", fontSize: 20) == 0)
        #expect(font.lineWidth(text: "ll", fontSize: 20) > font.lineWidth(text: "l", fontSize: 20))
        let (small, large) = (font.lineWidth(text: "Hello", fontSize: 10), font.lineWidth(text: "Hello", fontSize: 40))
        #expect(abs(large - 4 * small) < 1e-6)
    }

    // MARK: Outlines

    @Test func aGlyphsOutlineIsTheFontsOwnWithYPointingDown() throws {
        let layout = try font().layout(of: "l")
        let outline = try #require(layout.glyphs.first?.outline)

        // "l" spans x 52...171 and y -20...825 above the baseline (glyf table).
        let xs = stride(from: 0, to: outline.coords.count, by: 2).map { outline.coords[$0] }
        let ys = stride(from: 1, to: outline.coords.count, by: 2).map { outline.coords[$0] }
        #expect(abs(xs.min()! - 52) < 1)
        #expect(abs(xs.max()! - 171) < 1)
        #expect(abs(ys.min()! - -825) < 1, "the top of the glyph is above the baseline: negative y")
        #expect(abs(ys.max()! - 20) < 1)
        #expect(layout.width == 225)
    }

    @Test func glyphsFollowEachOtherByTheirAdvance() throws {
        let layout = try font().layout(of: "l l")

        #expect(layout.glyphs.count == 2, "a space has no outline")
        #expect(layout.glyphs[0].x == 0)
        #expect(layout.glyphs[1].x == 225 + 400, "after \"l\" and the space")
    }

    @Test func anOutlineIsMadeOncePerGlyph() throws {
        let font = try #require(Excalifont(bundle: Bundle(for: Excalifont.self)))

        _ = font.layout(of: "lll")
        #expect(font.cachedGlyphCount == 1)
        _ = font.layout(of: "llll")
        #expect(font.cachedGlyphCount == 1, "drawn again, not made again")
        _ = font.layout(of: "H")
        #expect(font.cachedGlyphCount == 2)
    }

    @Test func charactersTheFontLacksComeFromAFallbackFont() throws {
        let layout = try font().layout(of: "日")

        #expect(!layout.glyphs.isEmpty)
        #expect(layout.width > 0)
    }

    @Test func aLineOfTextIsOnePathInSceneCoordinates() throws {
        let font = try font()
        // "l" at 100 units per em ... drawn at size 100 with its anchor at (500, 300),
        // left-aligned: x from 500 + 5.2 to 500 + 17.1 (design 52...171 / 10).
        let line = GeometryTextLine(
            element: 0, version: 1, partIndex: 0, color: 0, opacity: 1, fontSize: 100, fontFamily: 5, align: 0,
            x: 500, y: 300, angle: 0, textStart: 0, textLength: 1)
        let path = TextLineOutline.path(of: line, text: "l", font: font)
        let xs = stride(from: 0, to: path.coords.count, by: 2).map { path.coords[$0] }
        let ys = stride(from: 1, to: path.coords.count, by: 2).map { path.coords[$0] }

        #expect(abs(xs.min()! - 505.2) < 0.1 && abs(xs.max()! - 517.1) < 0.1)
        #expect(abs(ys.min()! - (300 - 82.5)) < 0.1 && abs(ys.max()! - 302) < 0.1)

        // Right-aligned, the line ends at the anchor: its advance is 22.5.
        var right = line
        right.align = GeometryTextAlign.right.rawValue
        let rightPath = TextLineOutline.path(of: right, text: "l", font: font)
        let rightXs = stride(from: 0, to: rightPath.coords.count, by: 2).map { rightPath.coords[$0] }
        #expect(abs(rightXs.min()! - (500 - 22.5 + 5.2)) < 0.1)

        // Turned a quarter clockwise about the anchor, "up" points right.
        var turned = line
        turned.angle = .pi / 2
        let turnedPath = TextLineOutline.path(of: turned, text: "l", font: font)
        let turnedXs = stride(from: 0, to: turnedPath.coords.count, by: 2).map { turnedPath.coords[$0] }
        #expect(abs(turnedXs.max()! - 582.5) < 0.2, "the top of the glyph is now to the right of the anchor")
    }
}
