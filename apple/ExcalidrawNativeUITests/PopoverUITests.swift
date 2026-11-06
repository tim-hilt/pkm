import XCTest

/// The phone's Tool family picker: a custom popover that swallows Canvas touches (ADR-0013).
@MainActor final class PopoverUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenSampleDrawing"]
        app.launch()
        try XCTSkipUnless(app.buttons["Hand"].waitForExistence(timeout: 10))
        try XCTSkipIf(app.buttons["Zoom in"].exists, "the phone toolbar only")
    }

    private func openShapePicker() {
        app.buttons["Rectangle"].tap()
        app.buttons["Rectangle"].tap()
        XCTAssertTrue(app.buttons["Ellipse"].waitForExistence(timeout: 5), "the picker is open")
    }

    func testATapOnTheCanvasOnlyClosesThePicker() {
        openShapePicker()
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "family-picker"
        shot.lifetime = .keepAlways
        add(shot)

        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)).tap()

        let closed = NSPredicate(format: "exists == false")
        wait(for: [expectation(for: closed, evaluatedWith: app.buttons["Ellipse"])], timeout: 5)
        XCTAssertTrue(app.buttons["Rectangle"].isSelected, "the Tool is unchanged")
    }

    func testChoosingAToolClosesThePickerAndArmsIt() {
        openShapePicker()
        app.buttons["Ellipse"].tap()
        let closed = NSPredicate(format: "exists == false")
        wait(for: [expectation(for: closed, evaluatedWith: app.buttons["Diamond"])], timeout: 5)
        XCTAssertTrue(app.buttons["Ellipse"].isSelected)
    }
}
