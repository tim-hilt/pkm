import Foundation
import Testing

@testable import ExcalidrawNative

/// Keeping the Pencil-only drawing setting between launches: `CanvasController`
/// stores it once the core turned it on or the user chose, and restores it
/// into every editor. A new controller and editor on the same settings stand
/// for the next launch. What the mode does to input is tested in the core.
@MainActor @Suite struct PencilOnlyDrawingTests {
    @MainActor private struct Launch {
        let canvas = CanvasMetalView()
        let editor = Editor()
        let controller: CanvasController

        init(settings: UserDefaults) {
            controller = CanvasController(settings: settings)
            canvas.editor = editor
            controller.canvas = canvas
        }

        func tap(_ kind: PointerKind) {
            for phase in [PointerPhase.down, .up] {
                canvas.send(
                    PointerEvent(
                        id: 1, phase: phase, x: 300, y: 300, kind: kind, button: .primary, pressure: 0.5, timestamp: 0,
                        modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
            }
        }
    }

    @Test func theFirstPencilTurnsItOnForThisAndTheNextLaunch() {
        let settings = UserDefaults.isolated()
        let first = Launch(settings: settings)
        #expect(!first.controller.pencilOnlyDrawing)

        first.tap(.pencil)
        #expect(first.controller.pencilOnlyDrawing)

        let next = Launch(settings: settings)
        #expect(next.controller.pencilOnlyDrawing)
        #expect(next.editor.pencilOnlyDrawing())
    }

    @Test func fingersAloneStoreNothing() {
        let settings = UserDefaults.isolated()
        let launch = Launch(settings: settings)

        launch.tap(.finger)

        #expect(!launch.controller.pencilOnlyDrawing)
        #expect(settings.object(forKey: "pencilOnlyDrawing") == nil)
    }

    @Test func switchingItOffIsKeptAgainstLaterPencilUse() {
        let settings = UserDefaults.isolated()
        let first = Launch(settings: settings)
        first.tap(.pencil)

        first.controller.setPencilOnlyDrawing(false)
        first.tap(.pencil)
        #expect(!first.controller.pencilOnlyDrawing)

        let next = Launch(settings: settings)
        next.tap(.pencil)
        #expect(!next.controller.pencilOnlyDrawing)
        #expect(!next.editor.pencilOnlyDrawing())
    }

    @Test func switchingItOnBeforeAnyPencilIsKept() {
        let settings = UserDefaults.isolated()
        Launch(settings: settings).controller.setPencilOnlyDrawing(true)

        let next = Launch(settings: settings)
        #expect(next.controller.pencilOnlyDrawing)
    }
}
