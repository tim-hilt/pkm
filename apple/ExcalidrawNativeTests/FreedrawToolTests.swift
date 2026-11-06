import Foundation
import Testing

@testable import ExcalidrawNative

/// Drawing strokes through the Canvas: pointer input with pressure reaches the
/// core, and the stroke reaches the renderer while it is drawn and the Tool
/// controls afterwards. What a stroke is made of is tested in the core.
@MainActor @Suite struct FreedrawToolTests {
    @MainActor private struct Setup {
        let canvas = CanvasMetalView()
        let editor = Editor()
        let controller = CanvasController(settings: .isolated())
        var renderer: CanvasRenderer { canvas.renderer! }
    }

    private func makeSetup() throws -> Setup {
        let setup = Setup()
        _ = try #require(setup.canvas.renderer)
        setup.renderer.setGeometry(setup.editor.geometry())
        setup.canvas.editor = setup.editor
        setup.controller.canvas = setup.canvas
        setup.controller.setTool(.freedraw)
        return setup
    }

    private func pencil(_ phase: PointerPhase, _ x: Double, _ y: Double, pressure: Float) -> PointerEvent {
        PointerEvent(
            id: 1, phase: phase, x: x, y: y, kind: .pencil, button: .primary, pressure: pressure, timestamp: 0,
            modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false))
    }

    @Test func aStrokeReachesTheRendererWhileItIsDrawn() throws {
        let setup = try makeSetup()

        setup.canvas.send(pencil(.down, 0, 0, pressure: 0.2))
        setup.canvas.send(pencil(.move, 30, 10, pressure: 0.4))
        let early = setup.renderer.geometry
        setup.canvas.send(pencil(.move, 60, 0, pressure: 0.8))

        #expect(setup.renderer.geometry !== early)
        #expect((setup.renderer.geometry as? Geometry)?.partCount() ?? 0 > 0)
    }

    @Test func theToolStaysAfterAStroke() throws {
        let setup = try makeSetup()

        setup.canvas.send(pencil(.down, 0, 0, pressure: 0.2))
        setup.canvas.send(pencil(.move, 30, 10, pressure: 0.4))
        setup.canvas.send(pencil(.up, 60, 0, pressure: 0.3))

        #expect(setup.editor.elementCount() == 1)
        #expect(setup.controller.tool == .freedraw)
    }
}
