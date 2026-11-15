import XCTest

/// A Task's sheet: a Blocker opens on top of it, and Back returns to the Task; a Parent is picked in it.
@MainActor final class TasksUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenTasks"]
        app.launch()
        try XCTSkipUnless(app.staticTexts["Book the dentist"].waitForExistence(timeout: 10))
    }

    /// The form shown on top of the sheet, which is titled with its Task.
    private func showsForm(of title: String) -> Bool {
        app.navigationBars[title].waitForExistence(timeout: 5)
    }

    func testABlockerOpensOnTopAndBackReturns() {
        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))

        // The menu sits below the medium sheet's fold; a plain tap there doesn't open it.
        let addBlocker = app.buttons["task-add-blocker"]
        for _ in 0..<4 where !addBlocker.isHittable { app.collectionViews.firstMatch.swipeUp() }
        let candidate = app.buttons["Reply to Anna's email"].firstMatch
        for _ in 0..<3 where !candidate.exists {
            addBlocker.staticTexts.firstMatch.press(forDuration: 0.6)
            _ = candidate.waitForExistence(timeout: 2)
        }
        candidate.tap()

        app.buttons["Reply to Anna's email"].firstMatch.tap()
        XCTAssertTrue(showsForm(of: "Reply to Anna's email"))
        XCTAssertEqual(
            app.descendants(matching: .any)["task-title"].firstMatch.value as? String, "Reply to Anna's email")

        app.navigationBars.buttons["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))
        XCTAssertFalse(app.navigationBars["Reply to Anna's email"].exists)
    }

    func testTheBottomTabsSwitchTheLists() throws {
        try XCTSkipUnless(app.buttons["tasks-tab-backlog"].exists, "the Mac switches lists in its toolbar")
        app.buttons["tasks-tab-backlog"].tap()
        XCTAssertTrue(app.staticTexts["Learn Rust"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["Book the dentist"].exists)
        app.buttons["tasks-tab-active"].tap()
        XCTAssertTrue(app.staticTexts["Book the dentist"].waitForExistence(timeout: 5))
    }

    func testAParentIsSetInTheEditSheet() {
        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))

        let picker = app.buttons["task-parent"]
        for _ in 0..<4 where !picker.isHittable { app.collectionViews.firstMatch.swipeUp() }
        picker.tap()
        app.buttons["Reply to Anna's email"].firstMatch.tap()
        XCTAssertEqual(picker.label, "Parent, Reply to Anna's email")
    }
}
