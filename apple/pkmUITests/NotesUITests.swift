import XCTest

/// The Notes Context on a phone: the "+" is there from the Folder tree on, and opens a new Note.
@MainActor final class NotesUITests: XCTestCase {
    func testTheFolderTreesPlusOpensANewNote() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNotes"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Recipes"].waitForExistence(timeout: 10))
        try XCTSkipUnless(app.windows.firstMatch.horizontalSizeClass == .compact, "a phone pushes the Folder tree")

        app.buttons["add-note"].tap()

        XCTAssertTrue(app.descendants(matching: .any)["note-body"].waitForExistence(timeout: 5))
    }

    func testAFolderAndItsNoteOpenByTapping() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNotes"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Work"].waitForExistence(timeout: 10))
        try XCTSkipUnless(app.windows.firstMatch.horizontalSizeClass == .compact, "a phone pushes the Folder tree")

        app.staticTexts["Work"].tap()
        XCTAssertTrue(app.staticTexts["Weekly review"].waitForExistence(timeout: 5))
        app.staticTexts["Weekly review"].tap()

        XCTAssertTrue(app.descendants(matching: .any)["note-body"].waitForExistence(timeout: 5))
    }
}
