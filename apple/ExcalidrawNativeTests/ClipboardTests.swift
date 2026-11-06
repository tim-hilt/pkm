import Foundation
import Testing

@testable import ExcalidrawNative

/// Copying, cutting and pasting Elements through the Canvas: the text that
/// goes to and comes from the pasteboard is Excalidraw's clipboard JSON, made
/// and read by the core (tested there against what excalidraw.com writes).
/// The platform's pasteboard only carries the text, so these tests start at
/// the text.
@MainActor @Suite struct ClipboardTests {
    private static let drawing = Data(
        """
        {"type":"excalidraw","version":2,"elements":[
        {"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,"backgroundColor":"#ffc9c9","roundness":null}]}
        """.utf8)

    @MainActor private struct Setup {
        let canvas = CanvasMetalView()
        let editor = Editor()
        let controller = CanvasController(settings: .isolated())
    }

    private func makeSetup(drawing: Data? = nil) throws -> Setup {
        let setup = Setup()
        _ = try #require(setup.canvas.renderer)
        setup.editor.setViewSize(width: 800, height: 600)
        if let drawing { try setup.editor.load(bytes: drawing) }
        setup.canvas.editor = setup.editor
        setup.controller.canvas = setup.canvas
        return setup
    }

    /// Selects the rectangle with a tap.
    private func selectRectangle(_ canvas: CanvasMetalView) {
        for phase in [PointerPhase.down, .up] {
            canvas.send(
                PointerEvent(
                    id: 0, phase: phase, x: 150, y: 125, kind: .mouse, button: .primary, pressure: 0.5, timestamp: 0,
                    modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        }
    }

    @Test func nothingIsCopiedWithNothingSelected() throws {
        let setup = try makeSetup(drawing: Self.drawing)

        #expect(setup.canvas.clipboardText(cutting: false) == nil)
        #expect(!setup.canvas.canCopy)
    }

    @Test func theSelectionIsCopiedAsExcalidrawClipboardJSONAndPastesIntoAnotherCanvas() throws {
        let source = try makeSetup(drawing: Self.drawing)
        selectRectangle(source.canvas)
        #expect(source.canvas.canCopy)

        let text = try #require(source.canvas.clipboardText(cutting: false))

        #expect(text.contains(#""type":"excalidraw/clipboard""#), "\(text)")
        let target = try makeSetup()
        #expect(target.canvas.pasteClipboardText(text))
        #expect(target.editor.elementCount() == 1)
        #expect(target.editor.selection().count == 1)
        #expect(target.controller.canUndo)
    }

    @Test func textThatIsNotClipboardJSONIsNotPasted() throws {
        let setup = try makeSetup()

        #expect(!setup.canvas.pasteClipboardText("just some words"))

        #expect(setup.editor.elementCount() == 0)
    }

    @Test func cuttingGivesTheTextAndDeletesTheSelectionUndoably() throws {
        let setup = try makeSetup(drawing: Self.drawing)
        selectRectangle(setup.canvas)

        let text = setup.canvas.clipboardText(cutting: true)

        #expect(text != nil)
        #expect(setup.editor.selection().isEmpty)
        #expect(setup.canvas.clipboardText(cutting: false) == nil)
        setup.controller.undo()
        #expect(setup.editor.selection().count == 1)
    }

    @Test func aPasteIsPlacedAroundTheGivenPoint() throws {
        let source = try makeSetup(drawing: Self.drawing)
        selectRectangle(source.canvas)
        let text = try #require(source.canvas.clipboardText(cutting: false))
        let target = try makeSetup()

        #expect(target.canvas.pasteClipboardText(text, at: CGPoint(x: 100, y: 80)))

        let saved = String(decoding: target.editor.save(), as: UTF8.self)
        #expect(saved.contains(#""x": 50"#) && saved.contains(#""y": 55"#), "\(saved)")
    }
}
