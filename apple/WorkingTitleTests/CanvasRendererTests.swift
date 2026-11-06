import Foundation
import Metal
import Testing

@testable import WorkingTitle

/// Renders offscreen and probes pixels: the strokes must really be visible where the
/// core put them, at any zoom, in the style the core asked for.
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
        // (Hand-drawn shapes draw every edge twice, as Excalidraw does, so the
        // coverage of the edge pixels adds up to more than a half.)
        let (frame, _) = try render(viewport: Viewport(origin: [0, 0.5], zoom: 1))
        #expect((60...190).contains(frame.red(200, 98)))
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

    // MARK: Stroke styles

    private func render(rectangle extra: String) throws -> RenderedImage {
        try OffscreenRender.render(TestScenes.rectangle(extra: extra), viewport: Viewport()).image
    }

    @Test func dashedStrokesAlternateInkAndPaperAlongTheEdge() throws {
        // The top edge runs from x = 50: 8 on, 10 off, 8 on, ... (round caps
        // of 1.25 on top).
        let frame = try render(rectangle: #","strokeStyle":"dashed""#)

        #expect(frame.red(54, 100) < 60, "first dash")
        #expect(frame.red(63, 100) == 255, "first gap")
        #expect(frame.red(72, 100) < 60, "second dash")
        #expect(frame.red(81, 100) == 255, "second gap")
    }

    @Test func dottedStrokesAreSmallDotsFarApart() throws {
        // Dots 1.5 long (plus caps) every 9.5 units from x = 50.
        let frame = try render(rectangle: #","strokeStyle":"dotted""#)

        #expect(frame.red(51, 100) < 60, "first dot")
        #expect(frame.red(55, 100) == 255, "gap")
        #expect(frame.red(60, 100) < 60, "second dot")
    }

    @Test func translucentStrokesBlendOnceWhereTheySketchOverThemselves() throws {
        // Roughness 0 draws every edge twice on the very same line. At 50 %
        // opacity ink (30) over paper (255) is 142; blending twice would give 86.
        let frame = try render(rectangle: #","opacity":50"#)

        #expect((130...155).contains(frame.red(200, 100)), "\(frame.red(200, 100))")
        #expect((130...155).contains(frame.red(50, 200)), "left edge")
        #expect(frame.red(200, 190) == 255, "inside stays paper")
    }

    @Test func translucentStrokeJoinsAreNotDarkerThanTheRestOfTheLine() throws {
        // The roundness 3 corners are curves made of many overlapping capsules.
        let frame = try render(rectangle: #","opacity":50,"roundness":{"type":3}"#)

        for (x, y) in [(200, 100), (330, 190), (200, 280)] {
            #expect((130...155).contains(frame.red(x, y)), "edge at (\(x), \(y)): \(frame.red(x, y))")
        }
        // The middle of the top right corner's curve: from (298, 100) via
        // (330, 100) to (330, 132), at t = 1/2 that is (322, 108).
        #expect((130...155).contains(frame.red(322, 108)), "corner: \(frame.red(322, 108))")
    }

    // MARK: Caching

    @Test func replacingTheGeometryOfTheSameSceneRebuildsNothing() throws {
        let renderer = try OffscreenRender.makeRenderer()
        let editor = Editor()
        try editor.load(
            bytes: Data(
                TestScenes.drawing().replacingOccurrences(
                    of: #""roughness":0}]"#,
                    with: #""roughness":0},{"id":"b","type":"ellipse","width":50,"height":50}]"#
                ).utf8))
        renderer.setGeometry(editor.geometry())
        _ = OffscreenRender.draw(with: renderer)
        #expect(renderer.elementsBuilt == 2)

        renderer.setGeometry(editor.geometry())
        _ = OffscreenRender.draw(with: renderer)

        #expect(renderer.elementsBuilt == 2, "versions are unchanged")
    }

    @Test func loadingAnotherDrawingRebuildsEverything() throws {
        let renderer = try OffscreenRender.makeRenderer()
        renderer.setGeometry(TestScenes.rectangle())
        _ = OffscreenRender.draw(with: renderer)
        // The same handle and version, but another Element.
        renderer.setGeometry(TestScenes.rectangle(extra: ##","strokeColor":"#e03131""##))
        let frame = OffscreenRender.draw(with: renderer)

        #expect(renderer.elementsBuilt == 2)
        #expect(frame.red(200, 100) > 200, "red ink, not the cached dark one")
    }

    @Test func zoomingToAnotherLevelFlattensAgainButPanningDoesNot() throws {
        let renderer = try OffscreenRender.makeRenderer()
        renderer.setGeometry(TestScenes.rectangle())
        _ = OffscreenRender.draw(with: renderer)
        renderer.viewport = Viewport(origin: [30, 30], zoom: 1)
        _ = OffscreenRender.draw(with: renderer)
        #expect(renderer.elementsBuilt == 1, "panned")

        renderer.viewport = Viewport(origin: [30, 30], zoom: 4)
        _ = OffscreenRender.draw(with: renderer)
        #expect(renderer.elementsBuilt == 2, "zoomed in")
    }

    // MARK: Stable handles

    private func drawRectangle(_ editor: Editor, at x: Double) {
        let modifiers = Modifiers(shift: false, ctrl: false, alt: false, meta: false)
        editor.setTool(tool: .rectangle)
        for (phase, dx, time) in [(PointerPhase.down, 0.0, 0.0), (.move, 40, 0.05), (.up, 40, 0.1)] {
            editor.pointer(
                event: PointerEvent(
                    id: 0, phase: phase, x: x + dx, y: 40 + dx, kind: .mouse, button: .primary, pressure: 0.5,
                    timestamp: time,
                    modifiers: modifiers))
        }
    }

    @Test func addingOrDeletingOneElementOfALargeSceneRebuildsOnlyThatOne() throws {
        let renderer = try OffscreenRender.makeRenderer()
        let editor = Editor()
        let repository = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        try editor.load(bytes: try Data(contentsOf: repository.appending(path: "benchmarks/scene-5000.excalidraw")))
        renderer.setGeometry(editor.geometry())
        _ = OffscreenRender.draw(with: renderer)
        let loaded = renderer.elementsBuilt
        #expect(loaded > 1000)

        drawRectangle(editor, at: 100_000)
        let clock = ContinuousClock()
        let took = clock.measure {
            renderer.setGeometry(editor.geometry())
            _ = OffscreenRender.draw(with: renderer)
        }
        print("adding one Element: \(renderer.elementsBuilt - loaded) built, \(took)")
        #expect(renderer.elementsBuilt == loaded + 1, "only the new Element")

        editor.deleteSelection()
        renderer.setGeometry(editor.geometry())
        _ = OffscreenRender.draw(with: renderer)
        #expect(renderer.elementsBuilt == loaded + 1, "deleting builds nothing")
    }
}
