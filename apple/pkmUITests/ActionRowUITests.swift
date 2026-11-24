import XCTest

/// The phone Action row's popovers (ADR-0013): one tap switches, a Canvas touch only closes.
@MainActor final class ActionRowUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenSampleDrawing"]
        app.launch()
        try XCTSkipUnless(app.buttons["Hand"].waitForExistence(timeout: 10))
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "the phone toolbar only")
    }

    private func gone(_ element: XCUIElement) {
        wait(
            for: [expectation(for: NSPredicate(format: "exists == false"), evaluatedWith: element)], timeout: 5)
    }

    /// Arms the rectangle Tool: its style triggers appear without needing a hit on an Element.
    private func openStroke() {
        app.buttons["Rectangle"].tap()
        XCTAssertTrue(app.buttons["Stroke"].waitForExistence(timeout: 5))
        app.buttons["Stroke"].tap()
        XCTAssertTrue(app.staticTexts["Shades"].waitForExistence(timeout: 5), "the colour picker is open")
    }

    /// Draws a rectangle in the sample's empty lower part, which leaves it selected.
    private func selectANewRectangle() {
        app.buttons["Rectangle"].tap()
        let window = app.windows.firstMatch
        window.coordinate(withNormalizedOffset: CGVector(dx: 0.15, dy: 0.68)).press(
            forDuration: 0.1, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.76)))
        XCTAssertTrue(app.buttons["Actions"].waitForExistence(timeout: 5), "the rectangle is selected")
    }

    func testWithAnElementSelectedTriggersSwitchInOneTapAndACanvasTouchKeepsTheSelection() {
        selectANewRectangle()
        app.buttons["Stroke"].tap()
        XCTAssertTrue(app.staticTexts["Shades"].waitForExistence(timeout: 5))
        app.buttons["Properties"].tap()
        XCTAssertTrue(app.staticTexts["Opacity"].waitForExistence(timeout: 5))
        gone(app.staticTexts["Shades"])

        // An empty spot of the Canvas: without a popover, a tap there clears the Selection.
        let empty = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.62))
        empty.tap()
        gone(app.staticTexts["Opacity"])
        XCTAssertTrue(app.buttons["Actions"].exists, "the Element is still selected")
        empty.tap()
        gone(app.buttons["Actions"])
    }

    func testInLandscapeDeleteAndDuplicateSitOnTheRowAndLeaveTheActionsPopover() {
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }
        Thread.sleep(forTimeInterval: 2)  // the rotation animation swallows a tap
        selectANewRectangle()
        let delete = app.buttons.matching(NSPredicate(format: "label == 'Delete'"))
        let duplicate = app.buttons.matching(NSPredicate(format: "label == 'Duplicate'"))
        XCTAssertEqual(delete.count, 1)
        XCTAssertEqual(duplicate.count, 1)

        app.buttons["Actions"].tap()
        XCTAssertTrue(app.staticTexts["Layers"].waitForExistence(timeout: 5))
        XCTAssertEqual(delete.count, 1, "Delete is on the row only")
        XCTAssertEqual(duplicate.count, 1, "Duplicate is on the row only")
    }

    func testTappingAnotherTriggerSwitchesPopoversInOneTap() {
        openStroke()
        app.buttons["Properties"].tap()
        XCTAssertTrue(app.staticTexts["Opacity"].waitForExistence(timeout: 5))
        gone(app.staticTexts["Shades"])
    }

    func testATapOnTheOpenTriggerClosesItAndACanvasTouchOnlyCloses() {
        openStroke()
        app.buttons["Stroke"].tap()
        gone(app.staticTexts["Shades"])

        app.buttons["Properties"].tap()
        XCTAssertTrue(app.staticTexts["Opacity"].waitForExistence(timeout: 5))
        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)).tap()
        gone(app.staticTexts["Opacity"])
        XCTAssertTrue(app.buttons["Rectangle"].isSelected, "the Tool is unchanged")
    }

    func testTheHexFieldStaysAboveTheKeyboardAndThePopoverStaysOpenWhenItCloses() throws {
        openStroke()
        let hex = app.textFields["Hex code"]
        let before = app.buttons["Rectangle"].frame.minY
        hex.tap()
        let keyboard = app.keyboards.firstMatch
        try XCTSkipUnless(keyboard.waitForExistence(timeout: 5), "a hardware keyboard shows none")
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "hex-keyboard"
        shot.lifetime = .keepAlways
        add(shot)
        XCTAssertLessThanOrEqual(hex.frame.maxY, keyboard.frame.minY, "the field is visible")
        XCTAssertLessThanOrEqual(app.buttons["Rectangle"].frame.maxY, keyboard.frame.minY, "the bar moved up")

        hex.typeText("\n")
        gone(keyboard)
        XCTAssertTrue(app.staticTexts["Shades"].exists, "the popover is still open")
        XCTAssertEqual(app.buttons["Rectangle"].frame.minY, before, accuracy: 1, "the layout is back")
    }

    func testInLandscapeTheHexFieldIsReachableAboveTheKeyboard() throws {
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }
        Thread.sleep(forTimeInterval: 2)  // the rotation animation swallows a tap
        openStroke()
        let hex = app.textFields["Hex code"]
        hex.tap()
        let keyboard = app.keyboards.firstMatch
        try XCTSkipUnless(keyboard.waitForExistence(timeout: 5), "a hardware keyboard shows none")
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "hex-keyboard-landscape"
        shot.lifetime = .keepAlways
        add(shot)
        XCTAssertTrue(hex.isHittable, "the popover scrolled the field into view")
        XCTAssertLessThanOrEqual(hex.frame.maxY, keyboard.frame.minY)
    }

    // VoiceOver's escape gesture has no XCUITest API (a hardware Escape key does not reach
    // the popover in the simulator); `PhonePopover` wires it with `.accessibilityAction(.escape)`.
}
