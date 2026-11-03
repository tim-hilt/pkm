import Foundation
import Testing

@testable import WorkingTitle

/// Undo and redo through the Canvas: the buttons' commands reach the core, the
/// undone Scene reaches the renderer, and the buttons' state follows the
/// editor. What is undone, and how, is tested in the core.
@MainActor @Suite struct UndoRedoTests {
    @MainActor private struct Setup {
        let canvas = CanvasMetalView()
        let editor = Editor()
        let controller = CanvasController()
        var renderer: CanvasRenderer { canvas.renderer! }
    }

    private func makeSetup() throws -> Setup {
        let setup = Setup()
        _ = try #require(setup.canvas.renderer)
        setup.renderer.setGeometry(setup.editor.geometry())
        setup.canvas.editor = setup.editor
        setup.controller.canvas = setup.canvas
        return setup
    }

    private func mouse(_ phase: PointerPhase, _ x: Double, _ y: Double) -> PointerEvent {
        PointerEvent(
            id: 0, phase: phase, x: x, y: y, kind: .mouse, pressure: 0.5, timestamp: 0,
            modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false))
    }

    private func drawRectangle(_ setup: Setup) {
        setup.controller.setTool(.rectangle)
        setup.canvas.send(mouse(.down, 10, 10))
        setup.canvas.send(mouse(.move, 110, 60))
        setup.canvas.send(mouse(.up, 110, 60))
    }

    @Test func theButtonsAreEnabledExactlyWhenThereIsSomethingToUndoOrRedo() throws {
        let setup = try makeSetup()
        #expect(!setup.controller.canUndo && !setup.controller.canRedo)

        drawRectangle(setup)
        #expect(setup.controller.canUndo && !setup.controller.canRedo)

        setup.controller.undo()
        #expect(!setup.controller.canUndo && setup.controller.canRedo)

        setup.controller.redo()
        #expect(setup.controller.canUndo && !setup.controller.canRedo)
    }

    @Test func undoingAndRedoingReachTheRenderer() throws {
        let setup = try makeSetup()
        drawRectangle(setup)
        let drawn = setup.renderer.geometry

        setup.controller.undo()
        #expect(setup.editor.elementCount() == 0)
        #expect(setup.renderer.geometry !== drawn)
        #expect((setup.renderer.geometry as? Geometry)?.partCount() == 0)

        setup.controller.redo()
        #expect(setup.editor.elementCount() == 1)
        #expect((setup.renderer.geometry as? Geometry)?.partCount() ?? 0 > 0)
    }

    @Test func undoingTakesTheSelectionFrameAwayWithTheShape() throws {
        let setup = try makeSetup()
        drawRectangle(setup)
        // Select the new rectangle by its edge, then undo drawing it.
        for phase in [PointerPhase.down, .up] { setup.canvas.send(mouse(phase, 10, 30)) }
        #expect(setup.editor.selection().count == 1)

        setup.controller.undo()

        #expect(setup.editor.selection().isEmpty)
        #expect((setup.renderer.overlay as? Geometry)?.partCount() ?? 0 == 0)
    }
}
