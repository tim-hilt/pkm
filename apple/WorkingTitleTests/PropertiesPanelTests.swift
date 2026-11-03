import Foundation
import Testing

@testable import WorkingTitle

/// The properties panel's commands through the Canvas: they reach the core,
/// the restyled Scene reaches the renderer, and the panel's state follows the
/// editor. What each property does is tested in the core.
@MainActor @Suite struct PropertiesPanelTests {
    private static let drawing = """
        {"type":"excalidraw","version":2,"elements":[
        {"id":"box","type":"rectangle","x":100,"y":100,"width":100,"height":50,"strokeWidth":2,\
        "strokeColor":"#1e1e1e","backgroundColor":"transparent","roundness":null,\
        "version":3,"versionNonce":7}]}
        """

    @MainActor private struct Setup {
        let canvas = CanvasMetalView()
        let editor = Editor()
        let controller = CanvasController()
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

    /// Selects the rectangle by tapping its edge.
    private func select(_ setup: Setup) {
        for phase in [PointerPhase.down, .up] {
            setup.canvas.send(
                PointerEvent(
                    id: 0, phase: phase, x: 100, y: 125, kind: .mouse, pressure: 0.5, timestamp: 0,
                    modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        }
    }

    @Test func withNothingSelectedTheCommandIsTheStyleOfTheNextElement() throws {
        let setup = try makeSetup()
        #expect(setup.controller.properties?.strokeColor == "#1e1e1e")

        setup.controller.setProperty(.strokeColor("#e03131"))

        #expect(setup.controller.properties?.strokeColor == "#e03131")
        #expect(setup.editor.properties().strokeColor == "#e03131")
    }

    @Test func aCommandRestylesTheSelectionAndCanBeUndone() throws {
        let setup = try makeSetup()
        select(setup)
        #expect(setup.editor.selection() == ["box"])
        setup.controller.setProperty(.strokeWidth(.bold))
        #expect(setup.controller.properties?.strokeWidth == .bold)
        #expect(setup.controller.canUndo)

        setup.controller.undo()

        #expect(setup.controller.properties?.strokeWidth == .medium)
    }
}
