import XCTest

/// A Task's sheet: a Blocker or a Subtask's Parent opens on top of it, and Back returns to the Task.
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

    func testASubtasksParentOpensOnTopOrGoesBackWhenJustBelow() {
        let parent = app.buttons["task-parent"]
        func openParent() {
            for _ in 0..<4 where !parent.isHittable { app.collectionViews.firstMatch.swipeUp() }
            parent.tap()
        }
        // Opened from the list: the Parent goes on top, Back returns.
        app.staticTexts["Find the oat milk"].tap()
        XCTAssertTrue(showsForm(of: "Find the oat milk"))
        openParent()
        XCTAssertTrue(showsForm(of: "Buy milk"))
        app.navigationBars.buttons["Find the oat milk"].tap()
        XCTAssertTrue(showsForm(of: "Find the oat milk"))

        // Opened from its Parent: the Parent link goes back to it.
        app.swipeDown(velocity: .fast)
        XCTAssertTrue(app.navigationBars["Find the oat milk"].waitForNonExistence(timeout: 5))
        app.staticTexts["Buy milk"].firstMatch.tap()  // the row, before its Subtask's Parent marker
        XCTAssertTrue(showsForm(of: "Buy milk"))
        let subtask = app.collectionViews.staticTexts["Find the oat milk"].firstMatch
        for _ in 0..<4 where !subtask.isHittable { app.collectionViews.firstMatch.swipeUp() }
        subtask.tap()
        XCTAssertTrue(showsForm(of: "Find the oat milk"))
        openParent()
        XCTAssertTrue(showsForm(of: "Buy milk"))
        XCTAssertFalse(app.navigationBars["Buy milk"].buttons["Find the oat milk"].exists)
    }

    func testTheListSwitchSwitchesTheLists() throws {
        // A row of buttons in the bottom bar on iPhone and iPad.
        app.buttons["Backlog"].tap()
        XCTAssertTrue(app.staticTexts["Learn Rust"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["Book the dentist"].exists)
        app.buttons["Active"].tap()
        XCTAssertTrue(app.staticTexts["Book the dentist"].waitForExistence(timeout: 5))
    }

    func testAFavoriteOpenedFromTheFiltersListIsPushedOnIt() {
        app.buttons["Filters"].tap()
        app.staticTexts["@backlog"].tap()
        XCTAssertTrue(app.staticTexts["Learn Rust"].waitForExistence(timeout: 5))
        app.staticTexts["Learn Rust"].tap()  // its sheet shows over the pushed Filter
        XCTAssertTrue(showsForm(of: "Learn Rust"))
        app.swipeDown(velocity: .fast)
        XCTAssertTrue(app.navigationBars["Learn Rust"].waitForNonExistence(timeout: 5))
        app.navigationBars["Backlog"].buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.staticTexts["@backlog"].waitForExistence(timeout: 5))
    }

    func testASubtaskIsAddedFromTheSubtasksSection() {
        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))

        let add = app.buttons["add-subtask"]
        for _ in 0..<4 where !add.isHittable { app.collectionViews.firstMatch.swipeUp() }
        add.tap()
        let field = app.descendants(matching: .any)["subtask-title"].firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        // The field takes the focus as it appears, but its accessibility representation hides
        // that from `typeText`, so the keyboard is tapped.
        for key in ["C", "a", "l", "l"] { app.keys[key].tap() }
        app.buttons["Return"].tap()
        XCTAssertTrue(app.staticTexts["Call"].waitForExistence(timeout: 5))
        XCTAssertTrue(field.exists, "the field stays for the next Subtask")
    }

    func testAPriorityIsSetFromARowsContextMenu() {
        app.staticTexts["Book the dentist"].press(forDuration: 1)
        XCTAssertTrue(app.buttons["Reject"].waitForExistence(timeout: 5))
        app.buttons["P1"].tap()
        XCTAssertTrue(app.buttons["Reject"].waitForNonExistence(timeout: 5), "the menu closes")

        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))
        XCTAssertTrue(app.buttons["P1"].isSelected)
    }

    func testATagIsToggledFromARowsContextMenu() {
        app.staticTexts["Book the dentist"].press(forDuration: 1)
        app.buttons["Tags…"].tap()
        let picker = app.navigationBars["Tags"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertFalse(picker.buttons["Done"].exists)  // a row's circle is "Done" too
        app.buttons["home"].firstMatch.tap()
        XCTAssertTrue(picker.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["health home"].waitForExistence(timeout: 5))
    }

    func testATagCreatedInTheQuickPickerIsAdded() {
        app.staticTexts["Book the dentist"].press(forDuration: 1)
        app.buttons["Tags…"].tap()
        let picker = app.navigationBars["Tags"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        let search = app.searchFields["Search or create"]
        // The keyboard opens only when search is tapped; a tap while the sheet still rises is lost.
        for _ in 0..<3 where !app.keys["e"].exists {
            search.tap()
            _ = app.keys["e"].waitForExistence(timeout: 2)
        }
        search.typeText("errand")
        app.buttons["Create Tag 'errand'"].tap()
        XCTAssertTrue(picker.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["health errand"].waitForExistence(timeout: 5))
    }

    func testAnImageIsCopiedFromItsMenu() {
        app.staticTexts["Buy milk"].firstMatch.tap()
        XCTAssertTrue(showsForm(of: "Buy milk"))
        let description = app.textViews["Description"]
        XCTAssertTrue(description.waitForExistence(timeout: 5))
        for _ in 0..<4 where description.frame.maxY > app.frame.maxY * 0.8 {
            app.collectionViews.firstMatch.swipeUp()
        }
        UIPasteboard.general.items = []

        // The sample's Image sits just above its last line.
        description.coordinate(withNormalizedOffset: CGVector(dx: 0.2, dy: 1))
            .withOffset(CGVector(dx: 0, dy: -60)).press(forDuration: 1)
        let copy = app.buttons["Copy Image to Clipboard"]
        XCTAssertTrue(copy.waitForExistence(timeout: 5))
        copy.tap()
        XCTAssertTrue(UIPasteboard.general.hasImages)
    }

    func testTagsArePickedInTheirOwnSheet() {
        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))

        let row = app.buttons["task-tags"]
        for _ in 0..<4 where !row.isHittable { app.collectionViews.firstMatch.swipeUp() }
        row.tap()
        XCTAssertTrue(app.navigationBars["Tags"].waitForExistence(timeout: 5))
        // The keyboard opens only when search is tapped; a tap while the sheet still rises is lost.
        for _ in 0..<3 where !app.keys["h"].exists {
            app.searchFields["Search or create"].tap()
            _ = app.keys["h"].waitForExistence(timeout: 2)
        }
        XCTAssertTrue(app.keys["h"].exists)
        for key in ["h", "o"] { app.keys[key].tap() }
        XCTAssertEqual(app.searchFields["Search or create"].value as? String, "ho")
        app.buttons["home"].firstMatch.tap()
        app.navigationBars["Tags"].buttons["Done"].tap()

        XCTAssertTrue(app.navigationBars["Book the dentist"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["task-tags"].staticTexts["home"].exists)
        XCTAssertTrue(app.buttons["task-tags"].staticTexts["health"].exists)
    }
}

/// A Grouped Filter: the Tasks without a Group first, then a heading per Group-by Tag, even if empty.
@MainActor final class GroupedFilterUITests: XCTestCase {
    func testHeadingsShowEvenForAnEmptyGroup() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenGroupedFilter"]
        app.launch()
        XCTAssertTrue(app.staticTexts["Book the dentist"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["group-home"].exists)
        XCTAssertTrue(app.staticTexts["group-backlog"].exists)  // empty: the Query leaves it out
        XCTAssertFalse(app.staticTexts["Learn Rust"].exists)
    }
}
