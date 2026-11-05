import Foundation
import SwiftUI
import Testing

@testable import WorkingTitle

/// Keyboard shortcuts on the client: the menus (Mac menu bar, iPad shortcut
/// overlay) list each Command once with its first key, and keys the menus do
/// not list reach the core through the Canvas. Which key means what is
/// tested in the core.
@MainActor @Suite struct KeyboardShortcutsTests {
    private static let none = Modifiers(shift: false, ctrl: false, alt: false, meta: false)

    private func makeCanvas() -> (CanvasMetalView, CanvasController) {
        let canvas = CanvasMetalView()
        canvas.editor = Editor.withExcalifont()
        let controller = CanvasController(settings: .isolated())
        controller.canvas = canvas
        return (canvas, controller)
    }

    @Test func aKeyTheMenusDoNotListStillSelectsItsTool() {
        let (canvas, controller) = makeCanvas()
        #expect(canvas.pressKey("x", Self.none))
        #expect(controller.tool == .freedraw)
    }

    @Test func aKeyWithoutShortcutIsLeftToTheSystem() {
        let (canvas, _) = makeCanvas()
        #expect(!canvas.pressKey("b", Self.none))
    }

    @Test func menusListEachCommandOnceWithItsFirstKey() {
        let entries = MenuShortcut.all
        let commands = entries.map(\.command)
        #expect(Set(commands.map { "\($0)" }).count == commands.count)
        let freedraw = entries.first { $0.command == .setTool(.freedraw) }
        #expect(freedraw?.key == KeyEquivalent("p"))
        let delete = entries.first { $0.command == .delete }
        #expect(delete?.key == .delete)
        let toFront = entries.first { $0.command == .moveInZOrder(.toFront) }
        #expect(toFront?.modifiers == [.command, .option])
    }

    @Test func theSystemsEditItemsActOnTheCanvas() throws {
        let (canvas, controller) = makeCanvas()
        try canvas.editor?.load(
            bytes: Data(
                #"{"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","x":0,"y":0,"width":10,"height":10}]}"#
                    .utf8))
        canvas.selectAll(nil)
        #expect(controller.hasSelection)
        canvas.delete(nil)
        #expect(!controller.hasSelection)
        #expect(controller.canUndo)
    }

    @Test func keysTypeIntoATextBeingEditedInsteadOfPerformingShortcuts() {
        let (canvas, controller) = makeCanvas()
        controller.setTool(.text)
        for phase in [PointerPhase.down, .up] {
            canvas.send(
                PointerEvent(
                    id: 1, phase: phase, x: 300, y: 200, kind: .finger, pressure: 0.5, timestamp: 1,
                    modifiers: Self.none))
        }
        #expect(canvas.textOverlay.view != nil)
        // The menus disable their items, and the Canvas leaves keys alone.
        #expect(controller.isEditingText)
        #expect(!canvas.pressKey("r", Self.none))
        #expect(controller.tool != .rectangle)
        #expect(canvas.textOverlay.view != nil)
    }

    #if os(macOS)
        /// AppKit's `charactersIgnoringModifiers` keeps Shift: ⇧1 reads "!".
        @Test func shiftedDigitKeysReachTheirShortcut() throws {
            let event = try #require(
                NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: .shift, timestamp: 0, windowNumber: 0,
                    context: nil, characters: "!", charactersIgnoringModifiers: "!", isARepeat: false,
                    keyCode: 18))
            #expect(CanvasMetalView.keyName(of: event) == "1")
        }
    #endif
}
