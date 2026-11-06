import Foundation
import Testing

@testable import ExcalidrawNative

#if canImport(UIKit)
    import UIKit
#endif

/// The Action Bar's state and commands through the Canvas: where it is shown
/// is the core's decision (tested there), so these check that the chrome shows
/// it, follows it through panning and zooming, and that its commands reach the
/// core and the renderer.
@MainActor @Suite struct ActionBarTests {
    private static let drawing = """
        {"type":"excalidraw","version":2,"elements":[
        {"id":"box","type":"rectangle","x":100,"y":100,"width":100,"height":50,"strokeWidth":2,\
        "strokeColor":"#1e1e1e","backgroundColor":"transparent","roundness":null,\
        "version":3,"versionNonce":7}]}
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
        return setup
    }

    private func pointer(
        _ setup: Setup, _ phase: PointerPhase, at point: (Double, Double), time: Double = 0,
        kind: PointerKind = .mouse
    ) {
        setup.canvas.send(
            PointerEvent(
                id: 0, phase: phase, x: point.0, y: point.1, kind: kind, button: .primary, pressure: 0.5,
                timestamp: time, modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
    }

    /// Selects the rectangle by tapping its edge.
    private func select(_ setup: Setup) {
        pointer(setup, .down, at: (100, 125))
        pointer(setup, .up, at: (100, 125))
    }

    @Test func theBarIsShownWithTheSelectionAndHiddenWithout() throws {
        let setup = try makeSetup()
        #expect(setup.controller.actionBar == nil)

        select(setup)
        #expect(setup.controller.actionBar != nil)

        pointer(setup, .down, at: (400, 400))
        pointer(setup, .up, at: (400, 400))
        #expect(setup.controller.actionBar == nil)
    }

    @Test func theBarFollowsThePanOfTheCanvas() throws {
        let setup = try makeSetup()
        select(setup)
        let before = try #require(setup.controller.actionBar)

        setup.canvas.send(
            ScrollEvent(
                x: 0, y: 0, dx: 10, dy: 20,
                modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))

        let after = try #require(setup.controller.actionBar)
        #expect(after.x == before.x + 10)
        #expect(after.y == before.y + 20)
    }

    @Test func duplicatingShowsTheCopyAndSelectsIt() throws {
        let setup = try makeSetup()
        select(setup)

        setup.controller.duplicate()

        #expect(setup.editor.elementCount() == 2)
        #expect(setup.editor.selection().count == 1)
        #expect(setup.editor.selection() != ["box"])
        #expect(setup.controller.actionBar != nil)
        #expect(setup.controller.canUndo)
    }

    @Test func deletingRemovesTheSelectionAndHidesTheBar() throws {
        let setup = try makeSetup()
        select(setup)

        setup.controller.deleteSelection()

        #expect(setup.editor.elementCount() == 0)
        #expect(setup.controller.actionBar == nil)

        setup.controller.undo()

        #expect(setup.editor.elementCount() == 1)
        #expect(setup.controller.actionBar != nil)
    }

    /// Where `id` is in the saved file: later in the text is further up.
    private func position(of id: String, in setup: Setup) throws -> String.Index {
        let file = try #require(String(data: setup.editor.save(), encoding: .utf8))
        return try #require(file.range(of: "\"id\": \"\(id)\"")).lowerBound
    }

    @Test func zOrderCommandsReachTheCoreAndAreUndoable() throws {
        let setup = try makeSetup()
        select(setup)
        setup.controller.duplicate()
        // The copy is on top and selected; send it to the back.
        let copy = try #require(setup.editor.selection().first)

        setup.controller.moveInZOrder(.toBack)
        #expect(try position(of: copy, in: setup) < position(of: "box", in: setup))

        setup.controller.undo()
        #expect(try position(of: "box", in: setup) < position(of: copy, in: setup))
    }

    @Test func groupingAndUngroupingReachTheCoreAndAreUndoable() throws {
        let setup = try makeSetup()
        select(setup)
        setup.controller.duplicate()
        // The copy is selected; add the original to the Selection with Shift.
        setup.canvas.send(shiftPointer(.down, at: (100, 125)))
        setup.canvas.send(shiftPointer(.up, at: (100, 125)))
        #expect(setup.editor.selection().count == 2)
        #expect(setup.controller.actionBar?.canGroup == true)
        #expect(setup.controller.actionBar?.canUngroup == false)

        setup.controller.group()
        #expect(setup.controller.actionBar?.canGroup == false)
        #expect(setup.controller.actionBar?.canUngroup == true)

        setup.controller.ungroup()
        #expect(setup.controller.actionBar?.canUngroup == false)

        setup.controller.undo()
        #expect(setup.controller.actionBar?.canUngroup == true)
    }

    private func shiftPointer(_ phase: PointerPhase, at point: (Double, Double)) -> PointerEvent {
        PointerEvent(
            id: 0, phase: phase, x: point.0, y: point.1, kind: .mouse, button: .primary, pressure: 0.5,
            timestamp: 0, modifiers: Modifiers(shift: true, ctrl: false, alt: false, meta: false))
    }

    @Test func aLongPressOpensNoContextMenu() throws {
        let setup = try makeSetup()
        select(setup)

        // Held on empty Canvas by a finger, which a deliberate tap is not.
        pointer(setup, .down, at: (400, 400), time: 0, kind: .finger)
        pointer(setup, .up, at: (400, 400), time: 2, kind: .finger)

        // Nothing came of it: the Selection and the bar are as they were.
        #expect(setup.editor.selection() == ["box"])
        #expect(setup.controller.actionBar != nil)
    }
}
