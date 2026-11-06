import Foundation
import Testing

@testable import WorkingTitle

/// Erasing through the Canvas: pointer input reaches the core, what is about
/// to be erased reaches the renderer faded while the pointer is down, and the
/// Tool stays active afterwards. Which Elements an erase touches is tested in
/// the core.
@MainActor @Suite struct EraserToolTests {
    private static let drawing = """
        {"type":"excalidraw","version":2,"elements":[
        {"id":"box","type":"rectangle","x":100,"y":100,"width":100,"height":50,"strokeWidth":2,\
        "backgroundColor":"transparent","roundness":null,"version":3,"versionNonce":7}]}
        """

    @MainActor private struct Setup {
        let canvas = CanvasMetalView()
        let editor = Editor()
        let controller = CanvasController(settings: .isolated())
        var renderer: CanvasRenderer { canvas.renderer! }
    }

    private func makeSetup() throws -> Setup {
        let setup = Setup()
        _ = try #require(setup.canvas.renderer)
        try setup.editor.load(bytes: Data(Self.drawing.utf8))
        setup.renderer.setGeometry(setup.editor.geometry())
        setup.canvas.editor = setup.editor
        setup.controller.canvas = setup.canvas
        setup.controller.setTool(.eraser)
        return setup
    }

    private func mouse(_ phase: PointerPhase, _ x: Double, _ y: Double) -> PointerEvent {
        PointerEvent(
            id: 1, phase: phase, x: x, y: y, kind: .mouse, button: .primary, pressure: 0.5, timestamp: 0,
            modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false))
    }

    @Test func whatIsAboutToBeErasedReachesTheRendererBeforeItIsDeleted() throws {
        let setup = try makeSetup()
        let before = setup.renderer.geometry

        setup.canvas.send(mouse(.down, 150, 80))
        setup.canvas.send(mouse(.move, 150, 170))

        #expect(setup.renderer.geometry !== before)
        #expect(setup.editor.elementCount() == 1)
    }

    @Test func releasingDeletesAndTheToolStays() throws {
        let setup = try makeSetup()

        setup.canvas.send(mouse(.down, 150, 80))
        setup.canvas.send(mouse(.move, 150, 170))
        setup.canvas.send(mouse(.up, 150, 170))

        #expect(setup.editor.elementCount() == 0)
        #expect(setup.controller.tool == .eraser)
    }
}
