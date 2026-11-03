import Foundation
import Testing

@testable import WorkingTitle

#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

/// Creating and editing text through the Canvas: taps reach the core, the
/// native text view appears where and as the core says and follows panning
/// and zooming, and what is typed in it reaches the core. Where text goes and
/// when it is kept is tested in the core.
@MainActor @Suite struct TextEditingTests {
    private static let drawing = """
        {"type":"excalidraw","version":2,"elements":[
        {"id":"hello","type":"text","x":100,"y":50,"width":50,"height":25,
        "text":"Hello","originalText":"Hello","fontSize":20,"fontFamily":5,"lineHeight":1.25,
        "textAlign":"left","verticalAlign":"top","containerId":null,"autoResize":true,
        "version":3,"versionNonce":7}]}
        """

    @MainActor private struct Setup {
        let canvas = CanvasMetalView()
        let editor = Editor.withExcalifont()
        let controller = CanvasController()
        var overlay: TextEditorOverlay { canvas.textOverlay }
    }

    private func makeSetup() throws -> Setup {
        let setup = Setup()
        _ = try #require(setup.canvas.renderer)
        try setup.editor.load(bytes: Data(Self.drawing.utf8))
        setup.canvas.renderer?.setGeometry(setup.editor.geometry())
        setup.canvas.editor = setup.editor
        setup.controller.canvas = setup.canvas
        return setup
    }

    private func tap(_ setup: Setup, _ x: Double, _ y: Double, at time: Double) {
        for (phase, delay) in [(PointerPhase.down, 0.0), (.up, 0.05)] {
            setup.canvas.send(
                PointerEvent(
                    id: 1, phase: phase, x: x, y: y, kind: .finger, pressure: 0.5, timestamp: time + delay,
                    modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        }
    }

    /// What the text view says after the user typed `text` into it.
    private func type(_ text: String, in setup: Setup) {
        setup.overlay.text = text
        setup.overlay.changed()
    }

    @Test func tappingWithTheTextToolOpensTheTextViewWhereTheCoreSaysAndFocusesIt() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.text)
        #expect(setup.overlay.view == nil)

        tap(setup, 300, 200, at: 1)

        let shown = try #require(setup.editor.textEditor())
        let view = try #require(setup.overlay.view)
        #expect(view.superview === setup.canvas)
        // The box of the text, with room for the caret on both sides.
        let room = TextEditorOverlay.caretRoom
        #expect(
            view.frame == CGRect(x: shown.x - room, y: shown.y, width: shown.width + 2 * room, height: shown.height))
        #expect(shown.y == 187.5)
    }

    @Test func theTextViewIsSetInExcalifontAtTheFontSizeAndLineHeightOfTheText() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.text)
        setup.controller.setProperty(.fontSize(.large))
        tap(setup, 300, 200, at: 1)

        let shown = try #require(setup.editor.textEditor())
        let attributes = TextEditorOverlay.attributes(of: shown)
        let font = try #require(attributes[.font] as? PlatformFont)
        let paragraph = try #require(attributes[.paragraphStyle] as? NSParagraphStyle)

        #expect(font.pointSize == 28)
        #expect(font.fontName == Excalifont.shared?.postScriptName)
        // 28 points at a line height of 1.25.
        #expect(paragraph.minimumLineHeight == 35)
        #expect(paragraph.maximumLineHeight == 35)
    }

    @Test func whatIsTypedReachesTheCoreAndTheTextViewFollowsItsSize() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.text)
        tap(setup, 300, 200, at: 1)

        type("ll", in: setup)

        // Excalifont's "l" advances 225 units of 1000: two at 20 points.
        let shown = try #require(setup.editor.textEditor())
        #expect(shown.text == "ll")
        #expect(abs(shown.width - 9) < 1e-9)
        let view = try #require(setup.overlay.view)
        #expect(abs(view.frame.width - (9 + 2 * TextEditorOverlay.caretRoom)) < 1e-9)
    }

    @Test func theTextViewKeepsWhatIsTypedWhileItIsPlacedAgain() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.text)
        tap(setup, 300, 200, at: 1)

        type("ll", in: setup)
        setup.controller.setProperty(.fontSize(.extraLarge))

        #expect(setup.overlay.text == "ll")
        #expect(setup.editor.textEditor()?.fontSize == 36)
    }

    @Test func theTextViewFollowsZoomingWithTheTextAndFontSize() throws {
        let setup = try makeSetup()
        // Zooming acts around the middle of the view.
        setup.editor.setViewSize(width: 800, height: 600)
        setup.controller.setTool(.text)
        tap(setup, 300, 200, at: 1)
        let before = try #require(setup.overlay.view).frame

        setup.controller.zoomIn()

        let shown = try #require(setup.editor.textEditor())
        let view = try #require(setup.overlay.view)
        #expect(shown.zoom == 1.1)
        #expect(view.frame != before)
        #expect(abs(view.frame.minY - shown.y) < 1e-9)
        #expect(abs(shown.fontSize - 22) < 1e-9)
    }

    @Test func committingKeepsTheTextAndTakesTheTextViewAway() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.text)
        tap(setup, 300, 200, at: 1)
        type("Hi", in: setup)

        setup.overlay.onEnd?()

        #expect(setup.overlay.view == nil)
        #expect(setup.editor.textEditor() == nil)
        #expect(setup.editor.elementCount() == 2)
        #expect(setup.controller.canUndo)
    }

    @Test func emptyTextIsNotKept() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.text)
        tap(setup, 300, 200, at: 1)

        setup.overlay.onEnd?()

        #expect(setup.overlay.view == nil)
        #expect(setup.editor.elementCount() == 1)
        #expect(!setup.controller.canUndo)
    }

    @Test func aTapElsewhereOnTheCanvasCommitsAndTakesTheTextViewAway() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.text)
        tap(setup, 300, 200, at: 1)
        type("Hi", in: setup)

        tap(setup, 600, 500, at: 3)

        #expect(setup.overlay.view == nil)
        #expect(setup.editor.elementCount() == 2)
    }

    @Test func aDoubleTapOnTextOpensTheTextViewWithItsText() throws {
        let setup = try makeSetup()

        tap(setup, 120, 60, at: 1)
        #expect(setup.overlay.view == nil)
        tap(setup, 120, 60, at: 1.2)

        #expect(setup.overlay.view != nil)
        #expect(setup.overlay.text == "Hello")
        #expect(setup.editor.textEditor()?.text == "Hello")
    }

    @Test func editingTextDoesNotDrawItTwice() throws {
        let setup = try makeSetup()
        let drawn = { (setup.canvas.renderer?.geometry as? Geometry)?.textLineCount() ?? 0 }
        #expect(drawn() == 1)

        tap(setup, 120, 60, at: 1)
        tap(setup, 120, 60, at: 1.2)
        #expect(drawn() == 0)

        setup.overlay.onEnd?()
        #expect(drawn() == 1)
    }

    @Test func fontSizeAndAlignmentReachTheCoreThroughThePanel() throws {
        let setup = try makeSetup()
        setup.controller.setTool(.text)
        #expect(setup.controller.properties?.hasTextOptions == true)

        setup.controller.setProperty(.fontSize(.large))
        setup.controller.setProperty(.textAlign(.center))

        #expect(setup.controller.properties?.fontSize == .large)
        #expect(setup.controller.properties?.textAlign == .center)
    }
}
