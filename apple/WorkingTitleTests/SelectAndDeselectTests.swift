import Foundation
import Testing

@testable import WorkingTitle

/// Selecting through the Canvas: taps reach the core, and the selection frame
/// the core describes reaches the renderer and the screen. What a tap means
/// is tested in the core.
@MainActor @Suite struct SelectAndDeselectTests {
    /// A filled rectangle at (100.5, 100.5), 100 x 50. The half units put the
    /// left side of its selection frame (4 units out, 1 wide) in the middle of
    /// a pixel column.
    private static let drawing = Data(
        """
        {"type":"excalidraw","version":2,"elements":[
        {"id":"a","type":"rectangle","x":100.5,"y":100.5,"width":100,"height":50,
         "backgroundColor":"#ffc9c9","strokeWidth":2,"roundness":null}]}
        """.utf8)

    @MainActor private struct Setup {
        let canvas = CanvasMetalView()
        let editor = Editor()
        var renderer: CanvasRenderer { canvas.renderer! }
    }

    private func makeSetup() throws -> Setup {
        let setup = Setup()
        _ = try #require(setup.canvas.renderer)
        try setup.editor.load(bytes: Self.drawing)
        setup.renderer.setGeometry(setup.editor.geometry())
        setup.canvas.editor = setup.editor
        return setup
    }

    private func tap(_ canvas: CanvasMetalView, _ x: Double, _ y: Double) {
        for phase in [PointerPhase.down, .up] {
            canvas.send(
                PointerEvent(
                    id: 0, phase: phase, x: x, y: y, kind: .mouse, pressure: 0.5, timestamp: 0,
                    modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        }
    }

    private func overlayPartCount(_ setup: Setup) -> UInt64 {
        (setup.renderer.overlay as? Geometry)?.partCount() ?? 0
    }

    @Test func aTapSelectsAndTheFrameReachesTheRenderer() throws {
        let setup = try makeSetup()
        #expect(overlayPartCount(setup) == 0)

        tap(setup.canvas, 150, 125)

        #expect(setup.editor.selection() == ["a"])
        // The frame, and a fill and a stroke for each of the 8 handles and the rotation handle.
        #expect(overlayPartCount(setup) == 1 + 9 * 2)
    }

    @Test func aTapOnEmptyCanvasTakesTheFrameAway() throws {
        let setup = try makeSetup()
        tap(setup.canvas, 150, 125)

        tap(setup.canvas, 350, 350)

        #expect(setup.editor.selection().isEmpty)
        #expect(overlayPartCount(setup) == 0)
    }

    @Test func panningKeepsTheOverlayObjectAndZoomingMakesANewOne() throws {
        let setup = try makeSetup()
        tap(setup.canvas, 150, 125)
        let framed = setup.renderer.overlay
        let none = Modifiers(shift: false, ctrl: false, alt: false, meta: false)

        setup.canvas.send(ScrollEvent(x: 0, y: 0, dx: 10, dy: 10, modifiers: none))
        #expect(setup.renderer.overlay === framed, "the frame lives in the Scene; a pan moves nothing")

        setup.canvas.perform { $0.zoomIn() }
        #expect(setup.renderer.overlay !== framed, "its width and distance depend on zoom")
    }

    @Test func theSelectionFrameIsDrawnInTheSelectionColourOverTheScene() throws {
        let setup = try makeSetup()
        tap(setup.canvas, 150, 125)
        let renderer = try OffscreenRender.makeRenderer()
        renderer.setGeometry(setup.editor.geometry())
        renderer.setOverlay(setup.editor.overlay())
        renderer.viewport = Viewport(origin: [0, 0], zoom: 1)

        let image = OffscreenRender.draw(with: renderer)

        // Left side of the frame at x = 96.5: all of pixel column 96 (at y = 110, clear of the handles). Excalidraw's
        // selection colour is #6965db.
        let pixel = image.pixel(96, 110)
        #expect(abs(Int(pixel.red) - 0x69) <= 2)
        #expect(abs(Int(pixel.green) - 0x65) <= 2)
        #expect(abs(Int(pixel.blue) - 0xdb) <= 2)
        // Outside the frame the paper is white, inside it the rectangle's fill.
        #expect(image.pixel(60, 125).red == 255)
        #expect(image.pixel(150, 125).red == 0xff)
        #expect(image.pixel(150, 125).green < 0xff)
    }
}
