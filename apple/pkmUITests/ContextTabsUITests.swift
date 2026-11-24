import XCTest

/// Each tab keeps its own pushed screens; Settings is a sheet over the screen it was opened from.
/// A phone pushes a Note over the list, so these run on a phone.
@MainActor final class ContextTabsUITests: XCTestCase {
    private var app: XCUIApplication!
    private var noteBody: XCUIElement { app.descendants(matching: .any)["note-body"] }

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenNotes"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Reading list"].waitForExistence(timeout: 10))
        try XCTSkipUnless(app.windows.firstMatch.horizontalSizeClass == .compact, "a phone pushes the editor")
        app.staticTexts["Reading list"].tap()
        XCTAssertTrue(noteBody.waitForExistence(timeout: 5))
    }

    func testSwitchingTabsAndBackKeepsThePushedNote() {
        app.tabBars.buttons["Tasks"].tap()
        XCTAssertTrue(app.buttons["add-filter"].waitForExistence(timeout: 10) || app.buttons["add-task"].exists)
        app.tabBars.buttons["Notes"].tap()
        XCTAssertTrue(noteBody.waitForExistence(timeout: 5), "the Note is still open")
    }

    func testTappingTheSelectedTabAgainReturnsToTheFirstScreen() {
        app.tabBars.buttons["Notes"].tap()
        XCTAssertTrue(noteBody.waitForNonExistence(timeout: 5), "the Note is popped")
        XCTAssertTrue(app.buttons["add-note"].waitForExistence(timeout: 5))
    }

    func testAPushedScreenHasNoGearAndSettingsClosesBackToTheList() {
        XCTAssertFalse(app.navigationBars.buttons["settings"].isHittable, "a pushed screen has no gear")
        app.navigationBars.buttons["BackButton"].tap()
        app.navigationBars.buttons["settings"].tap()
        app.buttons["close-settings"].tap()
        XCTAssertTrue(app.buttons["add-note"].waitForExistence(timeout: 5), "back on the list")
    }
}
