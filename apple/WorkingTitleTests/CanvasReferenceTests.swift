import Testing

@testable import WorkingTitle

/// Seam 2: what the Canvas draws, compared with stored reference images
/// (see docs/renderer-reference-images.md for how to re-record them).
@MainActor @Suite struct CanvasReferenceTests {
    private func render(
        _ viewport: Viewport, of geometry: Geometry = TestScenes.rectangle(), pixelsPerPoint: Float = 1,
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
}
