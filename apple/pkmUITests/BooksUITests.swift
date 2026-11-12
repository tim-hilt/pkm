import XCTest

/// The Books Gallery, with the sample Books in a temporary Library.
@MainActor final class BooksUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenBooks"]
        app.launch()
    }

    /// The confirmation is a lone Delete button next to the cell, like Photos'.
    func testDeletingAsksWithALoneDeleteButtonAtTheCell() {
        let cell = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'book-'")).firstMatch
        XCTAssertTrue(cell.waitForExistence(timeout: 10))
        let identifier = cell.identifier

        cell.press(forDuration: 1)
        app.buttons["delete"].tap()

        let confirm = app.buttons["delete-confirm"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "delete-confirmation"
        shot.lifetime = .keepAlways
        add(shot)
        XCTAssertTrue(
            (cell.frame.minX...cell.frame.maxX).contains(confirm.frame.midX), "the button is at the cell")
        let texts = app.staticTexts.matching(NSPredicate(format: "label CONTAINS[c] 'delete'"))
        XCTAssertEqual(texts.count, 0, "no title or message")

        confirm.tap()

        XCTAssertTrue(app.buttons[identifier].waitForNonExistence(timeout: 5), "the Book is gone")
    }
}
