import Testing

@testable import WorkingTitle

/// Seam 2: what the Canvas draws, compared with stored reference images
/// (see docs/renderer-reference-images.md for how to re-record them).
@MainActor @Suite struct CanvasReferenceTests {
    private func render(_ viewport: Viewport, pixelsPerPoint: Float = 1, size: Int = 400) throws -> RenderedImage {
        try OffscreenRender.render(
            Editor().geometry(), viewport: viewport, pixelsPerPoint: pixelsPerPoint, size: size
        ).image
    }

    @Test func tracerRectangle() throws {
        expectMatchesReference(try render(Viewport()), named: "tracer-rectangle")
    }

    @Test func tracerRectangleZoomedAndPanned() throws {
        expectMatchesReference(try render(Viewport(origin: [40, 90], zoom: 2)), named: "tracer-rectangle-zoomed")
    }

    @Test func tracerRectangleOnRetinaScreen() throws {
        expectMatchesReference(
            try render(Viewport(), pixelsPerPoint: 2, size: 800), named: "tracer-rectangle-retina")
    }
}
