import Metal
import Testing

@testable import WorkingTitle

/// Renders offscreen and probes pixels: the rectangle Element's bounds must really be
/// visible where the core put it, at any zoom.
@MainActor @Suite struct CanvasRendererTests {
    private func render(
        viewport: Viewport, pixelsPerPoint: Float = 1, size: Int = 400
    ) throws -> (RenderedImage, CanvasRenderer) {
        try OffscreenRender.render(
            TestScenes.rectangle(), viewport: viewport, pixelsPerPoint: pixelsPerPoint, size: size)
    }

    @Test func rectangleOutlineIsInkAndEverythingElseIsPaper() throws {
        let (frame, _) = try render(viewport: Viewport())

        // Rectangle (50,100)-(330,280), 2 units wide: ink on the edges...
        #expect(frame.red(200, 100) < 60, "top edge")
        #expect(frame.red(50, 200) < 60, "left edge")
        #expect(frame.red(330, 200) < 60, "right edge")
        #expect(frame.red(200, 280) < 60, "bottom edge")
        // ...a square corner, paper inside and outside.
        #expect(frame.red(50, 100) < 60, "top-left corner")
        #expect(frame.red(200, 190) == 255, "inside")
        #expect(frame.red(20, 20) == 255, "outside")
    }

    @Test func edgesAreAntiAliased() throws {
        // The top edge spans y = 99...101. Shifting the view by half a unit makes it
        // span 98.5...100.5, so pixel row 98 is half covered: neither paper nor ink.
        let (frame, _) = try render(viewport: Viewport(origin: [0, 0.5], zoom: 1))
        #expect((100...190).contains(frame.red(200, 98)))
    }

    @Test func zoomAndOriginMoveTheRectangle() throws {
        // Scene point (50, 100) at the view's origin, 2x: the left edge sits at x = 0
        // and the top edge at y = 0; the right edge at (330 - 50) * 2 = 560 is off-screen.
        let (frame, _) = try render(viewport: Viewport(origin: [40, 90], zoom: 2), size: 400)
        #expect(frame.red(20, 200) < 60, "left edge at (50 - 40) * 2")
        #expect(frame.red(200, 20) < 60, "top edge at (100 - 90) * 2")
    }

    @Test func retinaScaleGrowsTheRectangleInDevicePixels() throws {
        let (frame, _) = try render(viewport: Viewport(), pixelsPerPoint: 2, size: 800)
        #expect(frame.red(400, 200) < 60, "top edge at 100 points = 200 pixels")
        #expect(frame.red(100, 400) < 60, "left edge at 50 points = 100 pixels")
    }

    @Test func eachRenderedFrameIsCounted() throws {
        let (_, renderer) = try render(viewport: Viewport())
        #expect(renderer.framesRendered == 1)
    }

    @Test func changesRequestFramesButNeverProduceThem() throws {
        let device = try #require(MTLCreateSystemDefaultDevice())
        let renderer = try #require(CanvasRenderer(device: device))
        var requests = 0
        renderer.onNeedsDisplay = { requests += 1 }

        renderer.setGeometry(TestScenes.rectangle())
        renderer.viewport = Viewport(origin: [10, 0], zoom: 1)
        renderer.viewport = Viewport(origin: [10, 0], zoom: 1)  // unchanged

        #expect(requests == 2)
        #expect(renderer.framesRendered == 0)
    }
}
