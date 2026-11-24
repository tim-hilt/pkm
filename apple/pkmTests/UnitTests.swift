import Foundation
import Testing

@testable import pkm

/// "Set unit…" and "Reset unit" in `CanvasController`: what the menu items
/// enable and what the alert's text may be. What the Unit does is tested in the core.
@MainActor @Suite struct UnitTests {
    private let editor = Editor()
    private let canvas = CanvasMetalView()
    private let controller = CanvasController(settings: UserDefaults.isolated())

    init() throws {
        _ = try #require(canvas.renderer)
        canvas.editor = editor
        controller.canvas = canvas
        try editor.load(
            bytes: Data(
                ##"{"type":"excalidraw","elements":[{"id":"a","type":"rectangle","x":0,"y":0,"width":100,"height":50,"backgroundColor":"#ffc9c9"}]}"##
                    .utf8))
    }

    private func select() {
        for phase in [PointerPhase.down, .up] {
            canvas.send(
                PointerEvent(
                    id: 0, phase: phase, x: 50, y: 25, kind: .mouse, button: .primary, pressure: 0.5, timestamp: 0,
                    modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        }
    }

    @Test func settingNeedsASelectionAndResettingANonDefaultUnit() {
        #expect(!controller.canSetUnit)
        #expect(!controller.canResetUnit)

        select()
        controller.setUnitFromText()  // nothing asked yet: not a value
        controller.askUnit()
        #expect(controller.unitText == 100.formatted(.number.precision(.fractionLength(2))))

        controller.unitText = 4.formatted()
        controller.setUnitFromText()
        #expect(editor.unit() == 25)
        #expect(controller.canResetUnit)

        controller.resetUnit()
        #expect(editor.unit() == 1)
    }

    @Test func theAlertTakesOnlyNumbersAboveZero() {
        for text in ["", "abc", "0", "-2"] {
            controller.unitText = text
            #expect(controller.unitValue == nil, "\(text)")
        }
        controller.unitText = 2.5.formatted()  // in the user's locale, as the alert shows it
        #expect(controller.unitValue == 2.5)
    }
}
