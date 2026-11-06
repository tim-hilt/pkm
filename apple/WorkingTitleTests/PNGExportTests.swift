import Foundation
import Testing

@testable import WorkingTitle

/// PNG export: the Scene or the Selection, drawn at 2x by the Metal renderer
/// offscreen, with or without the paper behind it.
@MainActor @Suite struct PNGExportTests {
    /// Two 100 x 50 rectangles at (100, 100) and (300, 100).
    private func twoRectangles() throws -> Editor {
        let editor = Editor.withExcalifont()
        let rectangle = { (id: String, x: Int) in
            #"{"id":"\#(id)","type":"rectangle","x":\#(x),"y":100,"width":100,"height":50,"backgroundColor":"transparent","roundness":null}"#
        }
        let drawing = #"{"type":"excalidraw","version":2,"elements":[\#(rectangle("a", 100)),\#(rectangle("b", 300))]}"#
        try editor.load(bytes: Data(drawing.utf8))
        editor.setViewSize(width: 800, height: 600)
        return editor
    }

    private func alpha(_ image: RenderedImage, _ x: Int, _ y: Int) -> UInt8 {
        image.bgra[(y * image.width + x) * RenderedImage.bytesPerPixel + 3]
    }

    private func hasInk(_ image: RenderedImage) -> Bool {
        (0..<image.height).contains { y in
            (0..<image.width).contains { x in alpha(image, x, y) > 0 && image.red(x, y) < 128 }
        }
    }

    @Test func theSceneIsExportedAtTwiceItsSizeWithPadding() throws {
        let image = try #require(PNGExport.render(try twoRectangles(), scope: .scene, background: true))

        #expect(image.width == 640 && image.height == 140, "(300 + 2 * 10) x (50 + 2 * 10) at 2x")
        #expect(hasInk(image))
        #expect(image.pixel(2, 2) == (255, 255, 255) && alpha(image, 2, 2) == 255, "the padding is paper")
    }

    @Test func thePixelSizeIsRoundedAsExcalidrawRoundsIt() throws {
        let editor = Editor.withExcalifont()
        let drawing =
            #"{"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","x":0,"y":0,"width":100.2,"height":50.3,"roundness":null}]}"#
        try editor.load(bytes: Data(drawing.utf8))

        let image = try #require(PNGExport.render(editor, scope: .scene, background: true))

        #expect(image.width == 240 && image.height == 141, "Math.round of 240.4 x 140.6")
    }

    @Test func withoutBackgroundThePaperIsTransparent() throws {
        let image = try #require(PNGExport.render(try twoRectangles(), scope: .scene, background: false))

        #expect(alpha(image, 2, 2) == 0)
        #expect(alpha(image, image.width / 2, image.height / 2) == 0, "between the rectangles")
        #expect(hasInk(image))
    }

    @Test func theSelectionIsExportedAlone() throws {
        let editor = try twoRectangles()
        let modifiers = Modifiers(shift: false, ctrl: false, alt: false, meta: false)
        for (phase, time) in [(PointerPhase.down, 0.0), (.up, 0.05)] {
            editor.pointer(
                event: PointerEvent(
                    id: 0, phase: phase, x: 300, y: 125, kind: .mouse, button: .primary, pressure: 0.5, timestamp: time,
                    modifiers: modifiers))
        }
        #expect(editor.selection() == ["b"])

        let image = try #require(PNGExport.render(editor, scope: .selection, background: true))
        #expect(image.width == 240 && image.height == 140, "(100 + 2 * 10) x (50 + 2 * 10) at 2x")
        #expect(hasInk(image))

        #expect(PNGExport.render(try twoRectangles(), scope: .selection, background: true) == nil, "nothing selected")
    }

    @Test func theExportIsAPNG() throws {
        let data = try #require(PNGExport.pngData(try twoRectangles(), scope: .scene, background: false))

        let decoded = try #require(RenderedImage(pngData: data))
        #expect(decoded.width == 640)
        #expect(alpha(decoded, 2, 2) == 0)
    }
}
