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
        try XCTSkipIf(app.buttons["Zoom in"].exists, "the phone toolbar only")
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
