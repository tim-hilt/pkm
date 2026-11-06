import CoreGraphics
import Testing

@testable import ExcalidrawNative

/// Keyboard frame in, viewport out: the Canvas pans just enough that the
/// caret's line sits above the on-screen keyboard.
@Suite struct KeyboardAvoidanceTests {
    private let viewport = ViewportState(originX: 10, originY: 20, zoom: 2)
    /// iPhone 15 Pro: the keyboard covers y 510...852 of a 393 × 852 Canvas.
    private let keyboard = CGRect(x: 0, y: 510, width: 393, height: 342)

    private func caret(bottom: CGFloat) -> CGRect {
        CGRect(x: 50, y: bottom - 20, width: 2, height: 20)
    }

    @Test func aCaretBelowTheKeyboardPansUpByTheOverlapPlusTheMargin() throws {
        // Caret bottom 600 → must reach 510 − 16 = 494: 106 points up = 53 Scene units.
        let panned = try #require(
            KeyboardAvoidance.viewport(keyboard: keyboard, caret: caret(bottom: 600), current: viewport))
        #expect(panned == ViewportState(originX: 10, originY: 73, zoom: 2))
    }

    @Test func aCaretInTheMarginAboveTheKeyboardPans() throws {
        let panned = try #require(
            KeyboardAvoidance.viewport(keyboard: keyboard, caret: caret(bottom: 500), current: viewport))
        #expect(panned == ViewportState(originX: 10, originY: 23, zoom: 2))
    }

    @Test func growingTextPansFurther() throws {
        let first = try #require(
            KeyboardAvoidance.viewport(keyboard: keyboard, caret: caret(bottom: 520), current: viewport))
        // One more line (30 points) after the first pan: the caret is 30 lower again.
        let second = try #require(
            KeyboardAvoidance.viewport(keyboard: keyboard, caret: caret(bottom: 494 + 30), current: first))
        #expect(second.originY == first.originY + 15)
    }

    @Test func aCaretAlreadyClearOfTheKeyboardDoesNotPan() {
        #expect(KeyboardAvoidance.viewport(keyboard: keyboard, caret: caret(bottom: 494), current: viewport) == nil)
        #expect(KeyboardAvoidance.viewport(keyboard: keyboard, caret: caret(bottom: 100), current: viewport) == nil)
    }

    @Test func noKeyboardOverTheCanvasDoesNotPan() {
        #expect(KeyboardAvoidance.viewport(keyboard: nil, caret: caret(bottom: 800), current: viewport) == nil)
        #expect(KeyboardAvoidance.viewport(keyboard: .null, caret: caret(bottom: 800), current: viewport) == nil)
    }

    @Test func aFloatingKeyboardBesideOrAboveTheCaretDoesNotPan() {
        let floating = CGRect(x: 200, y: 300, width: 180, height: 120)
        // Beside it.
        #expect(KeyboardAvoidance.viewport(keyboard: floating, caret: caret(bottom: 400), current: viewport) == nil)
        // Below it: lifting would move the caret under it.
        let below = CGRect(x: 250, y: 600, width: 2, height: 20)
        #expect(KeyboardAvoidance.viewport(keyboard: floating, caret: below, current: viewport) == nil)
    }
}
