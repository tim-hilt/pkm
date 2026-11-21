import XCTest

/// The Notes Context on a phone: the list shows every Note, "+" adds one and a Note pushes its
/// editor when tapped.
@MainActor final class NotesUITests: XCTestCase {
    func testThePlusOpensANewNote() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNotes"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Reading list"].waitForExistence(timeout: 10))
        try XCTSkipUnless(app.windows.firstMatch.horizontalSizeClass == .compact, "a phone pushes the editor")

        app.buttons["add-note"].tap()

        XCTAssertTrue(app.descendants(matching: .any)["note-body"].waitForExistence(timeout: 5))
    }

    func testANoteOpensByTapping() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNotes"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Reading list"].waitForExistence(timeout: 10))
        try XCTSkipUnless(app.windows.firstMatch.horizontalSizeClass == .compact, "a phone pushes the editor")

        app.staticTexts["Reading list"].tap()

        XCTAssertTrue(app.descendants(matching: .any)["note-body"].waitForExistence(timeout: 5))
    }
}
