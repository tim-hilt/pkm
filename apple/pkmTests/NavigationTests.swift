import Foundation
import Testing

@testable import pkm

/// Input reaching the core through the bindings and the viewport coming back
/// to the renderer. What a gesture means is tested in the core.
@MainActor @Suite struct NavigationTests {
    private func makeCanvas() throws -> (CanvasMetalView, CanvasRenderer, any GeometrySource) {
        let canvas = CanvasMetalView()
        let renderer = try #require(canvas.renderer)
        canvas.editor = try SampleDrawing.makeEditor()
        let geometry = try #require(renderer.geometry)
        return (canvas, renderer, geometry)
    }

    private func finger(_ id: UInt64, _ phase: PointerPhase, _ x: Double, _ y: Double) -> PointerEvent {
        PointerEvent(
            id: id, phase: phase, x: x, y: y, kind: .finger, button: .primary, pressure: 0.5, timestamp: 0,
            modifiers: Modifiers(
                shift: false, ctrl: false, alt: false, meta: false))
    }

    @Test func twoFingersPanTheRenderersViewport() throws {
        let (canvas, renderer, _) = try makeCanvas()

        canvas.send(finger(1, .down, 100, 100))
        canvas.send(finger(2, .down, 200, 100))
        canvas.send(finger(1, .move, 130, 110))
        canvas.send(finger(2, .move, 230, 110))

        #expect(renderer.viewport == Viewport(origin: [-30, -10], zoom: 1))
    }

    @Test func pinchingZoomsTheRenderersViewport() throws {
        let (canvas, renderer, _) = try makeCanvas()

        canvas.send(finger(1, .down, 100, 100))
        canvas.send(finger(2, .down, 200, 100))
        canvas.send(finger(1, .move, 50, 100))
        canvas.send(finger(2, .move, 250, 100))

        #expect(renderer.viewport == Viewport(origin: [75, 50], zoom: 2))
    }

    @Test func trackpadScrollAndPinchReachTheRenderer() throws {
        let (canvas, renderer, _) = try makeCanvas()
        let none = Modifiers(shift: false, ctrl: false, alt: false, meta: false)

        canvas.send(ScrollEvent(x: 0, y: 0, dx: 10, dy: 20, modifiers: none))
        #expect(renderer.viewport == Viewport(origin: [-10, -20], zoom: 1))

        canvas.send(MagnifyEvent(x: 0, y: 0, magnification: 1))
        #expect(renderer.viewport.zoom == 2)
    }

    @Test func navigatingDoesNotReplaceTheCachedGeometry() throws {
        let (canvas, renderer, geometry) = try makeCanvas()

        canvas.send(finger(1, .down, 100, 100))
        canvas.send(finger(2, .down, 200, 100))
        canvas.send(finger(2, .move, 300, 100))

        #expect(renderer.viewport != Viewport())
        #expect(renderer.geometry === geometry)
    }

    @Test func zoomCommandsReachTheRenderer() throws {
        let (canvas, renderer, _) = try makeCanvas()
        canvas.frame = CGRect(x: 0, y: 0, width: 400, height: 200)
        let controller = CanvasController(settings: .isolated())
        controller.canvas = canvas

        controller.zoomIn()
        #expect(abs(renderer.viewport.zoom - 1.1) < 1e-6)

        controller.resetZoom()
        #expect(renderer.viewport.zoom == 1)
    }

    /// Excalidraw's reset button reads the zoom, rounded to whole percent.
    @Test func theZoomButtonShowsTheZoom() throws {
        let (canvas, _, _) = try makeCanvas()
        canvas.frame = CGRect(x: 0, y: 0, width: 400, height: 200)
        let controller = CanvasController(settings: .isolated())
        controller.canvas = canvas
        #expect(controller.zoomPercent == 100)

        controller.zoomIn()
        #expect(controller.zoomPercent == 110)
        canvas.send(MagnifyEvent(x: 0, y: 0, magnification: 0.004))
        #expect(controller.zoomPercent == 110)
    }

    @Test func theHandToolPansWithOneFinger() throws {
        let (canvas, renderer, _) = try makeCanvas()
        let controller = CanvasController(settings: .isolated())
        controller.canvas = canvas

        controller.setTool(.hand)
        canvas.send(finger(1, .down, 100, 100))
        canvas.send(finger(1, .move, 130, 110))

        #expect(renderer.viewport == Viewport(origin: [-30, -10], zoom: 1))
    }
}
