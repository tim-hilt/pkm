import XCTest

/// Holding a selection handle of a Note's Body at the edge of the screen scrolls the Note and
/// extends the selection with it.
@MainActor final class NoteAutoscrollUITests: XCTestCase {
    private var app: XCUIApplication!
    private var title: XCUIElement { app.textFields["note-title"] }

    /// A Note longer than the screen, its Body being edited.
    private func openLongNote() {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNote"]
        app.launch()
        let body = app.descendants(matching: .any)["note-body"]
        XCTAssertTrue(body.waitForExistence(timeout: 10))
        body.tap()
        body.typeText(String(repeating: "word\n", count: 50))
    }

    /// Selects the word at mid-screen and drags the handle at `handle` to `y`, holding it there.
    /// Returns where the Title was once the word was selected.
    @discardableResult private func select(thenDrag handle: CGVector, to y: CGFloat) -> CGFloat {
        let origin = app.windows.firstMatch.coordinate(withNormalizedOffset: .zero)
        origin.withOffset(CGVector(dx: 30, dy: 300)).doubleTap()
        sleep(1)
        let before = title.frame.minY
        origin.withOffset(handle).press(
            forDuration: 0.3, thenDragTo: origin.withOffset(CGVector(dx: handle.dx, dy: y)), withVelocity: .slow,
            thenHoldForDuration: 3)
        return before
    }

    func testHoldingTheStartHandleAtTheTopScrollsUp() {
        openLongNote()
        XCTAssertFalse(title.isHittable, "typing scrolled the Title away")

        select(thenDrag: CGVector(dx: 17, dy: 292), to: 110)

        XCTAssertTrue(title.isHittable, "scrolled up to the Title")
    }

    func testHoldingTheEndHandleAtTheKeyboardScrollsDown() {
        openLongNote()
        app.swipeDown(velocity: .fast)
        app.swipeDown(velocity: .fast)
        XCTAssertTrue(title.isHittable)

        let before = select(thenDrag: CGVector(dx: 46, dy: 312), to: app.keyboards.firstMatch.frame.minY - 5)

        XCTAssertLessThan(title.frame.minY, before - 400, "scrolled down well past the screen")
    }
}
