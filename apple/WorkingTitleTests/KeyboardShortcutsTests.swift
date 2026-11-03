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
        canvas.editor = Editor()
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
}
