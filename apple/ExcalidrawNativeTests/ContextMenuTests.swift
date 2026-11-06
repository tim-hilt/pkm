import Foundation
import Testing

@testable import ExcalidrawNative

#if canImport(UIKit)
    import UIKit

    /// The Element menu handed to the system's edit menu, through the Canvas:
    /// which menu opens and what a press selects is the core's decision (tested
    /// there), so these check the entries, their order and state, and that each
    /// does what the matching command does. The hold itself and the system's
    /// presentation are checked by hand.
    @MainActor @Suite struct ContextMenuTests {
        private static let drawing = """
            {"type":"excalidraw","version":2,"elements":[
            {"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,"strokeWidth":2,\
            "backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7},
            {"id":"b","type":"rectangle","x":300,"y":100,"width":100,"height":50,"strokeWidth":2,\
            "backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}]}
            """

        @MainActor private struct Setup {
            let canvas = CanvasMetalView()
            let editor = Editor()
        }

        private func makeSetup() throws -> Setup {
            let setup = Setup()
            _ = try #require(setup.canvas.renderer)
            try setup.editor.load(bytes: Data(Self.drawing.utf8))
            setup.canvas.editor = setup.editor
            // The Canvas reports its own (zero) size when it gets the editor.
            setup.editor.setViewSize(width: 800, height: 600)
            return setup
        }

        private func titles(_ menu: UIMenu) -> [String] { menu.children.map(\.title) }

        private func action(_ title: String, in menu: UIMenu) throws -> UIAction {
            try #require(menu.children.first { $0.title == title } as? UIAction)
        }

        private func press(_ setup: Setup, at point: CGPoint, kind: PointerKind = .finger) -> UIMenu? {
            setup.canvas.contextMenuForPress(at: point, kind: kind)
        }

        // MARK: Entries

        @Test func theElementMenuHasTheEntriesInOrderWithArrangeAsASubmenu() throws {
            let menu = try makeSetup().canvas.elementMenu(canGroup: false, canUngroup: false)

            #expect(titles(menu) == ["Cut", "Copy", "Paste", "Duplicate", "Group", "Ungroup", "Arrange", "Delete"])
            let arrange = try #require(menu.children.first { $0.title == "Arrange" } as? UIMenu)
            #expect(titles(arrange) == ["Bring to Front", "Bring Forward", "Send Backward", "Send to Back"])
            #expect(try action("Delete", in: menu).attributes.contains(.destructive))
        }

        @Test func groupAndUngroupAreEnabledAsTheCoreSays() throws {
            for (canGroup, canUngroup) in [(true, false), (false, true), (false, false)] {
                let menu = try makeSetup().canvas.elementMenu(canGroup: canGroup, canUngroup: canUngroup)

                #expect(try action("Group", in: menu).attributes.contains(.disabled) != canGroup)
                #expect(try action("Ungroup", in: menu).attributes.contains(.disabled) != canUngroup)
            }
        }

        // MARK: What a press opens

        @Test func pressingAnElementSelectsItAndGivesTheElementMenu() throws {
            let setup = try makeSetup()

            let menu = try #require(press(setup, at: CGPoint(x: 150, y: 125)))

            #expect(setup.editor.selection() == ["a"])
            #expect(titles(menu).first == "Cut")
        }

        @Test func groupIsEnabledForTwoSelectedElements() throws {
            let setup = try makeSetup()
            setup.editor.perform(command: .selectAll)

            let menu = try #require(press(setup, at: CGPoint(x: 150, y: 125)))

            #expect(try !action("Group", in: menu).attributes.contains(.disabled))
            #expect(try action("Ungroup", in: menu).attributes.contains(.disabled))
        }

        @Test func theCanvasMenuHasPasteAndSelectAll() throws {
            let menu = try makeSetup().canvas.canvasMenu()

            #expect(titles(menu) == ["Paste", "Select All"])
        }

        @Test func pressingEmptyCanvasGivesTheCanvasMenuAndKeepsTheSelection() throws {
            let setup = try makeSetup()
            _ = try #require(press(setup, at: CGPoint(x: 150, y: 125)))

            let menu = try #require(press(setup, at: CGPoint(x: 600, y: 500)))

            #expect(titles(menu) == ["Paste", "Select All"])
            #expect(setup.editor.selection() == ["a"])
            try action("Select All", in: menu).performWithSender(nil, target: nil)
            #expect(Set(setup.editor.selection()) == ["a", "b"])
        }

        @Test func nothingOpensForThePencil() throws {
            let setup = try makeSetup()

            #expect(press(setup, at: CGPoint(x: 150, y: 125), kind: .pencil) == nil)
            #expect(press(setup, at: CGPoint(x: 600, y: 500), kind: .pencil) == nil)
            #expect(setup.editor.selection().isEmpty)
        }

        // MARK: Paste

        @Test func pasteFromTheMenuLandsAroundThePressPoint() async throws {
            let setup = try makeSetup()
            setup.editor.perform(command: .selectAll)
            setup.canvas.copy(nil)
            let text = try #require(UIPasteboard.general.string)
            setup.editor.perform(command: .selectAll)
            setup.canvas.perform { $0.deleteSelection() }
            #expect(setup.editor.elementCount() == 0)

            _ = try #require(press(setup, at: CGPoint(x: 600, y: 500)))
            setup.canvas.paste(itemProviders: [NSItemProvider(object: text as NSString)])

            #expect(await eventually { setup.editor.elementCount() == 2 })
            // The two rectangles span 300 wide, centred on the press point.
            let saved = String(decoding: setup.editor.save(), as: UTF8.self)
            #expect(saved.contains(#""x": 450"#) || saved.contains(#""x": 450."#))
        }

        // MARK: What the entries do

        @Test func eachEntryDoesWhatTheMatchingCommandDoes() throws {
            let setup = try makeSetup()
            setup.editor.perform(command: .selectAll)
            var menu = try #require(press(setup, at: CGPoint(x: 150, y: 125)))

            try action("Group", in: menu).performWithSender(nil, target: nil)
            menu = try #require(press(setup, at: CGPoint(x: 150, y: 125)))
            #expect(try action("Ungroup", in: menu).attributes.contains(.disabled) == false)

            try action("Ungroup", in: menu).performWithSender(nil, target: nil)
            try action("Duplicate", in: menu).performWithSender(nil, target: nil)
            #expect(setup.editor.elementCount() == 4)

            try action("Delete", in: menu).performWithSender(nil, target: nil)
            #expect(setup.editor.elementCount() == 2)
        }

        @Test func arrangeMovesTheSelectionInZOrder() throws {
            let setup = try makeSetup()
            let menu = try #require(press(setup, at: CGPoint(x: 150, y: 125)))
            let arrange = try #require(menu.children.first { $0.title == "Arrange" } as? UIMenu)

            let toFront = try #require(arrange.children.first as? UIAction)
            toFront.performWithSender(nil, target: nil)

            let saved = String(decoding: setup.editor.save(), as: UTF8.self)
            let (a, b) = (saved.range(of: #""id": "a""#), saved.range(of: #""id": "b""#))
            #expect(try #require(b).lowerBound < #require(a).lowerBound)
        }

        @Test func copyPutsTheSelectionOnThePasteboardAndCutAlsoDeletesIt() throws {
            let setup = try makeSetup()
            let menu = try #require(press(setup, at: CGPoint(x: 150, y: 125)))

            try action("Copy", in: menu).performWithSender(nil, target: nil)
            #expect(UIPasteboard.general.string?.contains("excalidraw/clipboard") == true)
            #expect(setup.editor.elementCount() == 2)

            UIPasteboard.general.string = ""
            try action("Cut", in: menu).performWithSender(nil, target: nil)
            #expect(UIPasteboard.general.string?.contains("excalidraw/clipboard") == true)
            #expect(setup.editor.elementCount() == 1)
        }

        // MARK: Accessibility custom actions

        private func accessibilityNames(_ setup: Setup) -> [String] {
            setup.canvas.accessibilityCustomActions?.map(\.name) ?? []
        }

        private func runAction(_ name: String, on setup: Setup) throws {
            let action = try #require(setup.canvas.accessibleActions.first { $0.name == name })
            action.run(setup.canvas)
        }

        @Test func theCustomActionsAreTheElementMenusActionsInOrder() throws {
            let setup = try makeSetup()
            setup.editor.perform(command: .selectAll)

            #expect(
                accessibilityNames(setup) == [
                    "Cut", "Copy", "Paste", "Duplicate", "Group",
                    "Bring to Front", "Bring Forward", "Send Backward", "Send to Back", "Delete",
                ])
            #expect(setup.canvas.isAccessibilityElement)
        }

        @Test func groupAndUngroupAreOfferedOnlyWhereTheyApply() throws {
            let setup = try makeSetup()
            _ = try #require(press(setup, at: CGPoint(x: 150, y: 125)))

            // One Element: neither grouping nor ungrouping would do anything.
            #expect(!accessibilityNames(setup).contains("Group"))
            #expect(!accessibilityNames(setup).contains("Ungroup"))

            setup.editor.perform(command: .selectAll)
            #expect(accessibilityNames(setup).contains("Group"))
            #expect(!accessibilityNames(setup).contains("Ungroup"))

            setup.canvas.perform { $0.group() }
            #expect(!accessibilityNames(setup).contains("Group"))
            #expect(accessibilityNames(setup).contains("Ungroup"))
        }

        @Test func thereAreNoCustomActionsWithoutASelection() throws {
            let setup = try makeSetup()

            #expect(setup.canvas.accessibleActions.isEmpty)
            #expect(accessibilityNames(setup).isEmpty)
            #expect(!setup.canvas.isAccessibilityElement)
        }

        @Test func theActionsRunTheElementMenusCommands() async throws {
            let setup = try makeSetup()
            setup.editor.perform(command: .selectAll)

            try runAction("Duplicate", on: setup)
            #expect(setup.editor.elementCount() == 4)

            try runAction("Delete", on: setup)
            #expect(setup.editor.elementCount() == 2)

            setup.editor.perform(command: .selectAll)
            try runAction("Cut", on: setup)
            #expect(setup.editor.elementCount() == 0)
            #expect(UIPasteboard.general.string?.contains("excalidraw/clipboard") == true)
        }

        @Test func theArrangeActionsMoveTheSelectionInZOrder() throws {
            let setup = try makeSetup()
            _ = try #require(press(setup, at: CGPoint(x: 150, y: 125)))

            try runAction("Bring to Front", on: setup)

            let saved = String(decoding: setup.editor.save(), as: UTF8.self)
            let (a, b) = (saved.range(of: #""id": "a""#), saved.range(of: #""id": "b""#))
            #expect(try #require(b).lowerBound < #require(a).lowerBound)
        }

        @Test func theGroupActionsRunTheElementMenusCommands() throws {
            let setup = try makeSetup()
            setup.editor.perform(command: .selectAll)

            try runAction("Group", on: setup)
            #expect(accessibilityNames(setup).contains("Ungroup"))
            setup.editor.perform(command: .selectAll)
            #expect(setup.editor.selection().count == 2)

            try runAction("Ungroup", on: setup)
            #expect(!accessibilityNames(setup).contains("Ungroup"))
        }

        @Test func pasteFromTheActionsLandsInTheMiddleOfTheView() async throws {
            let setup = try makeSetup()
            setup.editor.perform(command: .selectAll)
            try runAction("Copy", on: setup)

            try runAction("Paste", on: setup)

            #expect(await eventually { setup.editor.elementCount() == 4 })
            // The two rectangles span 300 wide, centred on the view's middle.
            let saved = String(decoding: setup.editor.save(), as: UTF8.self)
            #expect(saved.contains(#""x": 250"#) || saved.contains(#""x": 250."#))
        }
    }
#endif
