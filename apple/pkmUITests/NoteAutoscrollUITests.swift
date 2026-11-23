import XCTest

/// Holding a selection handle of a Note's Body at the edge of the screen scrolls the Note and
/// extends the selection with it.
@MainActor final class NoteAutoscrollUITests: XCTestCase {
    private var app: XCUIApplication!
    private var title: XCUIElement { app.textFields["note-title"] }
    private var body: XCUIElement { app.descendants(matching: .any)["note-body"] }

    /// A Note longer than the screen, its Body being edited.
    private func openLongNote() {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNote"]
        app.launch()
        XCTAssertTrue(body.waitForExistence(timeout: 10))
        body.tap()
        let lines = Int(app.windows.firstMatch.frame.height / 20)
        body.typeText(String(repeating: "word\n", count: lines))
    }

    /// Selects a word halfway down to the keyboard and drags its start or end handle to `y`, holding
    /// it there. The Body's text starts at its left edge.
    private func select(thenDrag handle: SelectionHandle, to y: CGFloat) {
        let origin = app.windows.firstMatch.coordinate(withNormalizedOffset: .zero)
        let left = body.frame.minX
        let line = app.keyboards.firstMatch.frame.minY / 2
        origin.withOffset(CGVector(dx: left + 8, dy: line)).doubleTap()
        sleep(1)
        // UIKit draws the start handle's knob above the word, the end handle's below.
        let grab = handle == .start ? CGVector(dx: left, dy: line - 8) : CGVector(dx: left + 30, dy: line + 14)
        origin.withOffset(grab).press(
            forDuration: 0.3, thenDragTo: origin.withOffset(CGVector(dx: grab.dx, dy: y)), withVelocity: .slow,
            thenHoldForDuration: 3)
    }

    private enum SelectionHandle { case start, end }

    func testHoldingTheStartHandleAtTheTopScrollsUp() {
        openLongNote()
        XCTAssertFalse(title.isHittable, "typing scrolled the Title away")

        select(thenDrag: .start, to: 20)

        // Not `isHittable`: the edit menu may cover the Title.
        XCTAssertTrue(title.exists && title.frame.minY >= 0, "scrolled up to the Title")
    }

    func testHoldingTheEndHandleAtTheKeyboardScrollsDown() {
        openLongNote()
        for _ in 0..<5 where !title.isHittable { body.swipeDown(velocity: .fast) }
        XCTAssertTrue(title.isHittable)
        let before = title.frame.minY

        select(thenDrag: .end, to: app.keyboards.firstMatch.frame.minY - 5)

        // Far off screen, the Title may leave the accessibility tree.
        XCTAssertTrue(!title.exists || title.frame.minY < before - 400, "scrolled down well past the screen")
    }
}
