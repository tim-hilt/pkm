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

    // VoiceOver's escape gesture has no XCUITest API (a hardware Escape key does not reach
    // the popover in the simulator); `PhonePopover` wires it with `.accessibilityAction(.escape)`.
}
