import XCTest

/// The zoom controls on every layout: Excalidraw's group bottom left on a large
/// screen, a zoom menu beside the main menu on a phone.
@MainActor final class ZoomUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingOpenSampleDrawing"]
        app.launch()
        try XCTSkipUnless(app.buttons["Hand"].waitForExistence(timeout: 10))
    }

    private func shows(_ element: XCUIElement, _ value: String) {
        let shown = NSPredicate(format: "value == %@", value)
        wait(for: [expectation(for: shown, evaluatedWith: element)], timeout: 5)
    }

    func testTheGroupResetsAndZoomsToFit() throws {
        try XCTSkipIf(UIDevice.current.userInterfaceIdiom == .phone, "the large layout only")
        let reset = app.buttons["Reset zoom"]
        reset.tap()
        shows(reset, "100%")
        XCTAssertTrue(app.buttons["Zoom to fit"].exists)
    }

    func testThePhoneMenuResetsAndZoomsToContent() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "the phone layout only")
        let menu = app.buttons["zoom-menu"]
        menu.tap()
        app.buttons["Reset"].tap()
        shows(menu, "100%")
        menu.tap()
        XCTAssertTrue(app.buttons["Zoom to Content"].exists)
    }
}
