import Foundation
import Testing

@testable import ExcalidrawNative

/// Renders text offscreen and probes pixels: paper is red 255 and ink 30.
@MainActor @Suite struct TextRenderingTests {
    private func render(
        _ geometry: any GeometrySource, viewport: Viewport = Viewport(), size: Int = 400
    ) throws -> RenderedImage {
        try OffscreenRender.render(geometry, viewport: viewport, size: size).image
    }

    /// Pixels in the given rectangle (device pixels) that are clearly ink.
    private func ink(_ frame: RenderedImage, x: Range<Int>, y: Range<Int>) -> Int {
        y.reduce(0) { total, row in total + x.filter { frame.red($0, row) < 128 }.count }
    }

    private func drawing(_ elements: String...) -> Geometry {
        TestScenes.geometry(
            of: #"{"type":"excalidraw","version":2,"elements":[\#(elements.joined(separator: ","))]}"#)
    }

    private let hello = TestScenes.text("t", 50, 50, (100, 50), "Hello", #","fontSize":40,"lineHeight":1.25"#)

    @Test func textIsInkInsideItsBoxAndNothingOutside() throws {
        let frame = try render(drawing(hello))

        #expect(ink(frame, x: 50..<160, y: 50..<110) > 150, "the letters")
        #expect(ink(frame, x: 0..<400, y: 120..<400) == 0, "below the text")
        #expect(ink(frame, x: 170..<400, y: 0..<400) == 0, "to the right of the text")
    }

    @Test func textRightAlignedEndsAtTheRightEdgeOfItsBox() throws {
        let frame = try render(
            drawing(TestScenes.text("t", 50, 50, (300, 50), "Hi", #","fontSize":40,"textAlign":"right""#)))

        #expect(ink(frame, x: 50..<250, y: 0..<400) == 0, "the left of the box is empty")
        #expect(ink(frame, x: 250..<355, y: 50..<110) > 50, "the text is at the right")
    }

    @Test func textKeepsItsColourAndOpacity() throws {
        let frame = try render(
            drawing(TestScenes.text("t", 50, 50, (100, 50), "Hello", ##","fontSize":40,"strokeColor":"#e03131""##)))

        // #e03131 has red 224; the darkest pixels of the text are that and
        // never the default ink.
        let reds = (50..<160).flatMap { x in (50..<110).map { frame.red(x, $0) } }
        #expect(reds.min() == 224)
    }

    @Test func textIsDrawnInZOrderWithShapesAroundIt() throws {
        let cover =
            ##"{"id":"c","type":"rectangle","x":40,"y":40,"width":140,"height":80,"roughness":0,"backgroundColor":"#a5d8ff","fillStyle":"solid","strokeColor":"transparent"}"##

        let under = try render(drawing(hello, cover))
        #expect(ink(under, x: 50..<160, y: 50..<110) == 0, "a later shape hides the text")

        let over = try render(drawing(cover, hello))
        #expect(ink(over, x: 50..<160, y: 50..<110) > 150, "text over a shape is visible")
        #expect(over.red(45, 45) == 165, "and the shape is still there")
    }

    @Test func textIsSharpAtAnyZoom() throws {
        // The H of "Hello" at 20x: its strokes are many pixels thick, so a
        // crisp edge leaves few pixels in between ink and paper.
        let frame = try render(drawing(hello), viewport: Viewport(origin: [50, 55], zoom: 12), size: 400)

        let inked = ink(frame, x: 0..<400, y: 0..<400)
        let between = (0..<400).reduce(0) { total, y in
            total + (0..<400).filter { (60...220).contains(Int(frame.red($0, y))) }.count
        }
        #expect(inked > 5_000, "the glyph is big on screen")
        #expect(between * 4 < inked, "a thin fringe of anti-aliasing, not a blur: \(between) of \(inked)")
    }

    @Test func anArrowLeavesAGapForItsLabel() throws {
        // A straight arrow along y = 200 with a "." label at its middle
        // (200, 200): the gap is the label's box and 5 around, so at least
        // 7 either side of the middle, where the dot is not.
        let arrow =
            ##"{"id":"a","type":"arrow","x":50,"y":200,"width":300,"height":0,"points":[[0,0],[300,0]],"roughness":0,"strokeWidth":2,"endArrowhead":null,"boundElements":[{"type":"text","id":"t"}]}"##
        let label = TestScenes.text(
            "t", 0, 0, (5, 25), ".", #","containerId":"a","textAlign":"center","verticalAlign":"middle""#)

        let frame = try render(drawing(arrow, label))

        #expect(frame.red(150, 200) < 60, "the arrow away from its label")
        #expect(frame.red(194, 200) == 255, "left of the dot")
        #expect(frame.red(206, 200) == 255, "right of the dot")
    }

    @Test func textIsNotRebuiltWhileNothingChanges() throws {
        let renderer = try OffscreenRender.makeRenderer()
        renderer.setGeometry(drawing(hello))
        _ = OffscreenRender.draw(with: renderer)
        let built = renderer.elementsBuilt

        renderer.viewport = Viewport(origin: [10, 10], zoom: 1)
        _ = OffscreenRender.draw(with: renderer)

        #expect(built == 1)
        #expect(renderer.elementsBuilt == built, "panning keeps the glyph meshes")
    }
}
