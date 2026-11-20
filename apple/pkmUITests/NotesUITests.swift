import XCTest

/// The Notes Context on a phone: the top level lists its Folders and Notes, a Folder pushes its
/// own, and "+" and New Folder add to the Folder shown. On an iPad: a tap beside the tree's rows
/// selects the top level.
@MainActor final class NotesUITests: XCTestCase {
    func testTheTopLevelsPlusOpensANewNote() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNotes"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Recipes"].waitForExistence(timeout: 10))
        try XCTSkipUnless(app.windows.firstMatch.horizontalSizeClass == .compact, "a phone pushes the Folders")

        app.buttons["add-note"].tap()

        XCTAssertTrue(app.descendants(matching: .any)["note-body"].waitForExistence(timeout: 5))
    }

    func testAFolderAndItsNoteOpenByTapping() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNotes"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Work"].waitForExistence(timeout: 10))
        try XCTSkipUnless(app.windows.firstMatch.horizontalSizeClass == .compact, "a phone pushes the Folders")

        app.staticTexts["Work"].tap()
        XCTAssertTrue(app.staticTexts["Weekly review"].waitForExistence(timeout: 5))
        app.staticTexts["Weekly review"].tap()

        XCTAssertTrue(app.descendants(matching: .any)["note-body"].waitForExistence(timeout: 5))
    }

    func testATapBesideTheTreeSelectsTheTopLevel() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNotes"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Work"].waitForExistence(timeout: 10))
        try XCTSkipUnless(app.windows.firstMatch.horizontalSizeClass == .regular, "an iPad shows the tree")
        let work = app.cells.containing(.staticText, identifier: "Work").firstMatch
        work.tap()
        XCTAssertTrue(work.wait(for: \.isSelected, toEqual: true, timeout: 5))

        let window = app.windows.firstMatch
        let below = CGVector(dx: work.frame.midX - window.frame.minX, dy: window.frame.height * 0.8)
        window.coordinate(withNormalizedOffset: .zero).withOffset(below).tap()

        XCTAssertTrue(work.wait(for: \.isSelected, toEqual: false, timeout: 5))
    }

    func testAFolderIsMadeInsideThePushedFolder() throws {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNotes"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Work"].waitForExistence(timeout: 10))
        try XCTSkipUnless(app.windows.firstMatch.horizontalSizeClass == .compact, "a phone pushes the Folders")
        XCTAssertTrue(app.staticTexts["Reading list"].exists, "top-level Notes beside the Folders")
        XCTAssertFalse(app.staticTexts["Notes"].exists, "no Folder for the top level")

        app.staticTexts["Work"].tap()
        XCTAssertTrue(app.staticTexts["Plans"].waitForExistence(timeout: 5))
        app.buttons["add-folder"].tap()
        app.alerts.textFields.firstMatch.typeText("Q1")
        app.alerts.buttons["Create"].tap()

        XCTAssertTrue(app.navigationBars["Q1"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Q1"].waitForExistence(timeout: 5))
    }
}
