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

    /// The row of the Task `title` in the list.
    private func row(_ title: String) -> XCUIElement {
        app.otherElements.matching(identifier: "task-row").containing(.staticText, identifier: title).firstMatch
    }

    /// A trailing swipe offers Reject; outside a `List` only from iOS 27 on.
    func testARowSwipedLeftOffersReject() throws {
        guard #available(iOS 27, *) else { throw XCTSkip("swipe actions need a List before iOS 27") }
        row("Book the dentist").swipeLeft()
        XCTAssertTrue(app.buttons["Reject"].waitForExistence(timeout: 5))
    }

    /// Drag tests are slow and flaky (synthesized presses); they run only with `make test-ui SLOW=1`.
    private func skipUnlessSlow() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["PKM_SLOW_UI"] == "1", "slow; set SLOW=1")
    }

    /// Drags the row `title` from its middle to `offset` of the row `target`, after a press shorter
    /// than the context menu's, until `landed`. A synthesized press sometimes lifts the row late, so
    /// it holds long at the target, or scrolls instead (with `List` too), so it is tried up to three times.
    private func drag(_ title: String, to target: String, at offset: CGVector, until landed: () -> Bool) {
        for _ in 0..<3 {
            let to = row(target).coordinate(withNormalizedOffset: offset)
            row(title).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
                .press(forDuration: 0.5, thenDragTo: to, withVelocity: .slow, thenHoldForDuration: 2)
            if landed() { return }
        }
    }

    /// The middle of a row onto the middle of another makes it a Subtask there.
    func testATaskDroppedOntoAnotherBecomesItsSubtask() throws {
        try skipUnlessSlow()
        let parentMarker = row("Reply to Anna's email").staticTexts["Book the dentist"]
        drag("Reply to Anna's email", to: "Book the dentist", at: CGVector(dx: 0.5, dy: 0.5)) {
            parentMarker.waitForExistence(timeout: 2)
        }
        XCTAssertTrue(parentMarker.exists)
    }

    /// Onto the line between two rows, a Task only moves there.
    func testATaskDraggedBetweenRowsOnlyMoves() throws {
        try skipUnlessSlow()
        func isAbove() -> Bool { row("Reply to Anna's email").frame.minY < row("Book the dentist").frame.minY }
        drag("Reply to Anna's email", to: "Book the dentist", at: CGVector(dx: 0.5, dy: 0)) {
            _ = row("Reply to Anna's email").waitForExistence(timeout: 2)
            return isAbove()
        }
        XCTAssertTrue(isAbove())
        XCTAssertFalse(row("Reply to Anna's email").staticTexts["Book the dentist"].exists)
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

    /// Picks `list` from the title menu: the title on iPhone and iPad, a segmented control on the Mac.
    private func showList(_ list: String) {
        #if os(macOS)
            app.radioButtons[list].tap()
        #else
            let title = NSPredicate(format: "NOT identifier IN {'settings', 'sort-menu'}")
            app.navigationBars.firstMatch.buttons.matching(title).firstMatch.tap()
            app.buttons[list].firstMatch.tap()
        #endif
    }

    /// The trailing toolbar ends with Settings, after the Context's own Sort.
    func testTheSettingsGearFollowsTheSortMenu() throws {
        #if os(macOS)
            throw XCTSkip("a Mac has ⌘, and no gear")
        #endif
        let sort = app.navigationBars.buttons["sort-menu"]
        let gear = app.navigationBars.buttons["settings"]
        XCTAssertTrue(sort.waitForExistence(timeout: 5))
        XCTAssertLessThan(sort.frame.minX, gear.frame.minX)
    }

    func testTheListSwitchSwitchesTheLists() throws {
        showList("Backlog")
        XCTAssertTrue(app.staticTexts["Learn Rust"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.staticTexts["Book the dentist"].exists)
        showList("Active")
        XCTAssertTrue(app.staticTexts["Book the dentist"].waitForExistence(timeout: 5))
    }

    func testAFavoriteOpenedFromTheFiltersListIsPushedOnIt() {
        showList("Filters")
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
        // The Return key is "Return" on an iPhone and "Go" on an iPad.
        app.keyboards.buttons.matching(NSPredicate(format: "identifier IN {'Return', 'Go'}")).firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Call"].waitForExistence(timeout: 5))
        XCTAssertTrue(field.exists, "the field stays for the next Subtask")
    }

    /// On iPadOS 26 the focused Title once updated the menu bar without end, and the app hung.
    func testATaskIsAddedFromTheNewTaskSheet() {
        app.buttons["add-task"].tap()
        XCTAssertTrue(app.navigationBars["New Task"].waitForExistence(timeout: 5))
        // The field takes the focus as it appears; see `testASubtaskIsAddedFromTheSubtasksSection`.
        XCTAssertTrue(app.keys["C"].waitForExistence(timeout: 5))
        for key in ["C", "a", "l", "l"] { app.keys[key].tap() }
        app.navigationBars["New Task"].buttons["Add"].tap()
        app.navigationBars["New Task"].buttons["Cancel"].tap()
        XCTAssertTrue(app.staticTexts["Call"].waitForExistence(timeout: 5))
    }

    func testATaskGetsTheTagsPickedAboveTheKeyboardAndItsShortcutTags() {
        app.buttons["add-task"].tap()
        XCTAssertTrue(app.keys["C"].waitForExistence(timeout: 5))
        for key in ["C", "a", "l", "l", "space"] { app.keys[key].tap() }
        app.keys["more"].tap()
        app.keys["@"].tap()
        app.keys["more"].tap()
        for key in ["h", "o", "m", "e"] { app.keys[key].tap() }
        let tags = app.buttons["task-tags"]
        XCTAssertTrue(tags.waitForExistence(timeout: 5))
        tags.tap()
        let picker = app.navigationBars["Tags"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        app.buttons["health"].firstMatch.tap()
        picker.buttons["Done"].tap()
        XCTAssertTrue(picker.waitForNonExistence(timeout: 5))
        app.navigationBars["New Task"].buttons["Add"].tap()
        app.navigationBars["New Task"].buttons["Cancel"].tap()
        let call = app.staticTexts["Call"]
        XCTAssertTrue(call.waitForExistence(timeout: 5))
        call.tap()
        XCTAssertTrue(showsForm(of: "Call"))
        let added = app.buttons["task-tags"].label
        XCTAssertTrue(added.contains("health") && added.contains("home"), added)
    }

    func testAChipAboveTheKeyboardCompletesTheAtWord() {
        app.buttons["add-task"].tap()
        XCTAssertTrue(app.keys["C"].waitForExistence(timeout: 5))
        for key in ["C", "a", "l", "l", "space"] { app.keys[key].tap() }
        app.keys["more"].tap()
        app.keys["@"].tap()
        app.keys["more"].tap()
        app.keys["h"].tap()
        let chip = app.buttons.matching(identifier: "tag-choice").firstMatch
        XCTAssertTrue(chip.waitForExistence(timeout: 5))
        XCTAssertGreaterThan(chip.frame.minY, app.navigationBars["New Task"].frame.maxY)
        chip.tap()
        XCTAssertTrue(chip.waitForNonExistence(timeout: 5), "the word is complete")
        XCTAssertTrue(app.buttons["task-tags"].label.contains("h"))
    }

    func testASubtaskFieldShowsTheChipsToo() {
        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))
        let add = app.buttons["add-subtask"]
        for _ in 0..<4 where !add.isHittable { app.collectionViews.firstMatch.swipeUp() }
        add.tap()
        XCTAssertTrue(app.keys["C"].waitForExistence(timeout: 5))
        for key in ["C", "a", "l", "l", "space"] { app.keys[key].tap() }
        app.keys["more"].tap()
        app.keys["@"].tap()
        app.keys["more"].tap()
        app.keys["h"].tap()
        XCTAssertTrue(app.buttons.matching(identifier: "tag-choice").firstMatch.waitForExistence(timeout: 5))
    }

    func testAPriorityIsSetFromARowsContextMenu() {
        app.staticTexts["Book the dentist"].press(forDuration: 1)
        XCTAssertTrue(app.buttons["Reject"].waitForExistence(timeout: 5))
        app.buttons["P1"].tap()
        XCTAssertTrue(app.buttons["Reject"].waitForNonExistence(timeout: 5), "the menu closes")

        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))
        XCTAssertEqual(app.buttons["task-priority"].label, "P1")
    }

    func testATagIsToggledFromARowsContextMenu() {
        app.staticTexts["Book the dentist"].press(forDuration: 1)
        app.buttons["Tags…"].tap()
        let picker = app.navigationBars["Tags"]
        XCTAssertTrue(picker.waitForExistence(timeout: 5))
        XCTAssertFalse(picker.buttons["Done"].exists)  // a row's circle is "Done" too
        app.buttons["home"].firstMatch.tap()
        XCTAssertTrue(picker.waitForNonExistence(timeout: 5))
        XCTAssertTrue(row("Book the dentist").staticTexts["home"].waitForExistence(timeout: 5))
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
        XCTAssertTrue(row("Book the dentist").staticTexts["errand"].waitForExistence(timeout: 5))
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

    func testTheSheetsCircleMarksTheTaskDoneAndTheToastReverts() {
        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))

        app.buttons["task-done"].tap()
        let revert = app.buttons["Revert"]
        XCTAssertTrue(revert.waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["task-done"].label, "Revert Done")
        revert.tap()
        XCTAssertEqual(app.buttons["task-done"].label, "Done")
    }

    func testTheCloseButtonClosesTheSheetUnlessTheTitleIsEmpty() throws {
        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))
        let close = app.buttons["close-task"]
        guard close.waitForExistence(timeout: 5) else { throw XCTSkip("Mac has Done instead") }
        // The field's accessibility frame is empty, so the tap lands beside the circle, at the end of its line.
        let circle = app.buttons["task-done"]
        // A tap while the sheet still rises is lost.
        for _ in 0..<4 where !app.keyboards.firstMatch.exists {
            sleep(1)
            circle.coordinate(withNormalizedOffset: CGVector(dx: 1, dy: 0.5)).withOffset(CGVector(dx: 200, dy: 0)).tap()
            _ = app.keyboards.firstMatch.waitForExistence(timeout: 2)
        }
        // As in the Subtask test, the keyboard is tapped: `typeText` doesn't see the field's focus.
        for _ in 0..<16 { app.keys["delete"].tap() }
        XCTAssertFalse(close.isEnabled)
        app.keys["B"].tap()
        XCTAssertTrue(close.isEnabled)
        close.tap()
        XCTAssertTrue(close.waitForNonExistence(timeout: 5))
    }

    func testRejectInTheSheetsMenuRejectsTheTaskWithAToastToRevert() throws {
        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))
        let more = app.buttons["task-more"]
        guard more.waitForExistence(timeout: 5) else { throw XCTSkip("Mac has Done instead") }
        more.tap()
        app.buttons["Reject"].tap()
        XCTAssertTrue(app.staticTexts["Task Rejected"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Revert"].exists)
    }

    func testTagsArePickedInTheirOwnSheet() {
        app.staticTexts["Book the dentist"].tap()
        XCTAssertTrue(showsForm(of: "Book the dentist"))

        let row = app.buttons["task-tags"]
        for _ in 0..<4 where !row.isHittable { app.collectionViews.firstMatch.swipeUp() }
        row.tap()
        XCTAssertTrue(app.navigationBars["Tags"].waitForExistence(timeout: 5))
        // A tap while the sheet still rises is lost; typeText needs no software keyboard.
        let search = app.searchFields["Search or create"]
        for _ in 0..<3 where !((search.value(forKey: "hasKeyboardFocus") as? Bool) ?? false) { search.tap() }
        search.typeText("ho")
        XCTAssertEqual(search.value as? String, "ho")
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

/// The Tags page in Settings: a tap on a Tag offers its actions, a swipe only Delete.
@MainActor final class ManageTagsUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenSettings", "-UITestingOpenTasks"]  // the latter: sample Tags
        app.launch()
        let link = app.buttons["manage-tags"]
        try XCTSkipUnless(link.waitForExistence(timeout: 10))
        link.tap()
        XCTAssertTrue(app.navigationBars["Tags"].waitForExistence(timeout: 5))
    }

    /// The first Tag's row; the list's header is the cell before it.
    private var tag: XCUIElement { app.cells.element(boundBy: 1) }

    func testATapOffersTheTagsActions() {
        tag.tap()
        XCTAssertTrue(app.buttons["Change Color…"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Reset to 0"].exists)  // "Default Color" is disabled, so not exposed
        XCTAssertFalse(app.buttons["Delete"].exists)
    }

    func testASwipeOffersOnlyDelete() {
        tag.swipeLeft()
        XCTAssertTrue(app.buttons["Delete"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Edit"].exists)
        XCTAssertFalse(app.buttons["Reset to 0"].exists)
    }

    func testChangeColorOpensThePicker() {
        tag.tap()
        app.buttons["Change Color…"].tap()
        XCTAssertTrue(app.navigationBars["backlog"].firstMatch.waitForExistence(timeout: 5))
    }
}
