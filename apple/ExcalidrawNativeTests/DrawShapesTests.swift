import Foundation
import Testing

@testable import ExcalidrawNative

/// Drawing shapes through the Canvas: input reaches the core, and the drawn
/// shape reaches the renderer and the Tool controls. What a drag means is
/// tested in the core.
@MainActor @Suite struct DrawShapesTests {
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
        return setup
    }

    private func mouse(_ phase: PointerPhase, _ x: Double, _ y: Double) -> PointerEvent {
        PointerEvent(
            id: 0, phase: phase, x: x, y: y, kind: .mouse, button: .primary, pressure: 0.5, timestamp: 0,
            modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false))
    }

    private func drag(_ canvas: CanvasMetalView, from: (Double, Double), to: (Double, Double)) {
        canvas.send(mouse(.down, from.0, from.1))
        canvas.send(mouse(.move, to.0, to.1))
        canvas.send(mouse(.up, to.0, to.1))
    }

    @Test func aDraggedShapeReachesTheRenderer() throws {
        let setup = try makeSetup()
        let before = setup.renderer.geometry
        setup.controller.setTool(.rectangle)

        drag(setup.canvas, from: (10, 10), to: (110, 60))

        #expect(setup.editor.elementCount() == 1)
        #expect(setup.renderer.geometry !== before)
        #expect((setup.renderer.geometry as? Geometry)?.partCount() ?? 0 > 0)
    }

    @Test func theShapeFollowsThePointerWhileItIsDragged() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.ellipse)

        setup.canvas.send(mouse(.down, 0, 0))
        setup.canvas.send(mouse(.move, 20, 20))
        let early = setup.renderer.geometry
        setup.canvas.send(mouse(.move, 80, 80))

        #expect(setup.renderer.geometry !== early)
    }

    @Test func theToolControlsFollowTheEditorWhichReturnsToSelectionAfterDrawing() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.diamond)
        #expect(setup.controller.tool == .diamond)

        drag(setup.canvas, from: (0, 0), to: (40, 40))

        #expect(setup.controller.tool == .selection)
    }

    @Test func theToolLockKeepsTheToolAfterDrawing() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.rectangle)
        setup.controller.setToolLock(true)
        #expect(setup.controller.toolLock)

        drag(setup.canvas, from: (0, 0), to: (40, 40))
        drag(setup.canvas, from: (50, 0), to: (90, 40))

        #expect(setup.controller.tool == .rectangle)
        #expect(setup.editor.elementCount() == 2)
    }

    @Test func changesOfTheCanvasThatDoNotTouchTheSceneKeepTheCachedGeometry() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.rectangle)
        drag(setup.canvas, from: (0, 0), to: (40, 40))
        let drawn = setup.renderer.geometry

        setup.canvas.send(
            ScrollEvent(
                x: 0, y: 0, dx: 10, dy: 10, modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        setup.controller.setTool(.hand)

        #expect(setup.renderer.geometry === drawn)
    }

    @Test func aSavedDrawingWithAShapeIsStillAValidDrawing() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.rectangle)
        drag(setup.canvas, from: (10, 20), to: (110, 70))

        let reloaded = Editor()
        try reloaded.load(bytes: setup.editor.save())

        #expect(reloaded.elementCount() == 1)
        #expect(reloaded.save() == setup.editor.save())
    }
}
