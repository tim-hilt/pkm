import CoreGraphics
import Testing

@testable import pkm

/// Keyboard frame in, viewport out: a text the keyboard covers is moved to
/// the middle of the room between the top of the screen and the keyboard.
@Suite struct KeyboardAvoidanceTests {
    private let viewport = ViewportState(originX: 10, originY: 20, zoom: 2)
    /// iPhone 15 Pro: the keyboard covers y 510...852 of a 393 × 852 Canvas,
    /// below the status bar's 59 points. The room's middle is at 284.5.
    private let keyboard = CGRect(x: 0, y: 510, width: 393, height: 342)
    private let top: CGFloat = 59

    private func caret(bottom: CGFloat) -> CGRect {
        CGRect(x: 50, y: bottom - 20, width: 2, height: 20)
    }

    private func text(top: CGFloat, bottom: CGFloat) -> CGRect {
        CGRect(x: 40, y: top, width: 120, height: bottom - top)
    }

    private func avoid(text: CGRect, caretBottom: CGFloat, keyboard: CGRect?) -> ViewportState? {
        KeyboardAvoidance.viewport(
            keyboard: keyboard, top: top, text: text, caret: caret(bottom: caretBottom), current: viewport)
    }

    @Test func aCoveredTextMovesToTheMiddleOfTheRoomAboveTheKeyboard() throws {
        // Middle 580 → 284.5: 295.5 points up = 147.75 Scene units.
        let panned = try #require(avoid(text: text(top: 560, bottom: 600), caretBottom: 600, keyboard: keyboard))
        #expect(panned == ViewportState(originX: 10, originY: 167.75, zoom: 2))
    }

    @Test func aCaretInTheMarginAboveTheKeyboardMovesTheTextToo() throws {
        // Middle 490 → 284.5: 205.5 points up.
        let panned = try #require(avoid(text: text(top: 480, bottom: 500), caretBottom: 500, keyboard: keyboard))
        #expect(panned == ViewportState(originX: 10, originY: 122.75, zoom: 2))
    }

    @Test func aTextTallerThanTheRoomCentresTheCaretsLine() throws {
        // Caret middle 590 → 284.5: 305.5 points up.
        let panned = try #require(avoid(text: text(top: 100, bottom: 600), caretBottom: 600, keyboard: keyboard))
        #expect(panned == ViewportState(originX: 10, originY: 172.75, zoom: 2))
    }

    @Test func aCaretAlreadyClearOfTheKeyboardDoesNotPan() {
        #expect(avoid(text: text(top: 474, bottom: 494), caretBottom: 494, keyboard: keyboard) == nil)
        #expect(avoid(text: text(top: 80, bottom: 100), caretBottom: 100, keyboard: keyboard) == nil)
    }

    @Test func noKeyboardOverTheCanvasDoesNotPan() {
        #expect(avoid(text: text(top: 780, bottom: 800), caretBottom: 800, keyboard: nil) == nil)
        #expect(avoid(text: text(top: 780, bottom: 800), caretBottom: 800, keyboard: .null) == nil)
    }

    @Test func aFloatingKeyboardBesideOrAboveTheCaretDoesNotPan() {
        let floating = CGRect(x: 200, y: 300, width: 180, height: 120)
        // Beside it.
        #expect(avoid(text: text(top: 380, bottom: 400), caretBottom: 400, keyboard: floating) == nil)
        // Below it: lifting would move the caret under it.
        let below = CGRect(x: 250, y: 600, width: 2, height: 20)
        #expect(
            KeyboardAvoidance.viewport(
                keyboard: floating, top: top, text: below, caret: below, current: viewport) == nil)
    }
}
