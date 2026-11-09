import Testing

@testable import pkm

/// Seam 2: what the Canvas draws, compared with stored reference images
/// (see docs/renderer-reference-images.md for how to re-record them).
@MainActor @Suite struct CanvasReferenceTests {
    private func render(
        _ viewport: Viewport, of geometry: any GeometrySource = TestScenes.rectangle(), pixelsPerPoint: Float = 1,
        size: Int = 400
    ) throws -> RenderedImage {
        try OffscreenRender.render(geometry, viewport: viewport, pixelsPerPoint: pixelsPerPoint, size: size).image
    }

    @Test func rectangleBounds() throws {
        expectMatchesReference(try render(Viewport()), named: "rectangle-bounds")
    }

    @Test func rectangleBoundsZoomedAndPanned() throws {
        expectMatchesReference(try render(Viewport(origin: [40, 90], zoom: 2)), named: "rectangle-bounds-zoomed")
    }

    @Test func rectangleBoundsOnRetinaScreen() throws {
        expectMatchesReference(
            try render(Viewport(), pixelsPerPoint: 2, size: 800), named: "rectangle-bounds-retina")
    }

    /// The bundled sample: rotated box, deleted and unknown Elements not shown.
    @Test func sampleDrawing() throws {
        let geometry = try SampleDrawing.makeEditor().geometry()
        expectMatchesReference(try render(Viewport(), of: geometry, size: 700), named: "sample-drawing")
    }

    /// Dashed, dotted, translucent, thick, rounded and every sloppiness.
    @Test func handDrawnStyles() throws {
        let geometry = TestScenes.handDrawnStyles()
        expectMatchesReference(try render(Viewport(), of: geometry, size: 420), named: "hand-drawn-styles")
    }

    /// Hachure, cross-hatch, zigzag and solid, plus architect and thick variants.
    @Test func fillStyles() throws {
        expectMatchesReference(try render(Viewport(), of: TestScenes.fillStyles(), size: 420), named: "fill-styles")
    }

    /// Ellipse, diamond, rounded and rotated rectangle, translucent solid.
    @Test func filledShapes() throws {
        expectMatchesReference(try render(Viewport(), of: TestScenes.filledShapes(), size: 420), named: "filled-shapes")
    }

    /// Concave and self-intersecting outlines by the non-zero and even-odd rules.
    @Test func fillRules() throws {
        expectMatchesReference(try render(Viewport(), of: TestScenes.fillRules(), size: 400), named: "fill-rules")
    }

    /// Straight, multi-point, curved, closed, rotated and dashed lines and arrows, and arrowheads.
    @Test func linesAndArrows() throws {
        expectMatchesReference(
            try render(Viewport(), of: TestScenes.linesAndArrows(), size: 520), named: "lines-and-arrows")
    }

    /// Strokes with simulated and real pressure, thick, translucent, rotated, and filled loops.
    @Test func freedraw() throws {
        expectMatchesReference(try render(Viewport(), of: TestScenes.freedraw(), size: 400), named: "freedraw")
    }

    /// Text in every alignment, size and family, rotated and translucent, and labels in shapes.
    @Test func text() throws {
        expectMatchesReference(try render(Viewport(), of: TestScenes.text(), size: 520), named: "text")
    }

    /// The same text four times as large: glyph outlines, not bitmaps.
    @Test func textZoomed() throws {
        expectMatchesReference(
            try render(Viewport(origin: [250, 20], zoom: 3), of: TestScenes.text(), size: 600), named: "text-zoomed")
    }
}
