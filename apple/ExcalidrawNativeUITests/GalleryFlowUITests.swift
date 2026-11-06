import XCTest

/// Gallery → Canvas → back, against an empty temporary
/// Drawings folder.
@MainActor final class GalleryFlowUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUp() {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-UITestingTemporaryStore"]
        app.launch()
    }

    func testCreatingADrawingOpensTheCanvasAndBackReturnsToTheGalleryWithIt() {
        let newDrawing = app.buttons["new-drawing"]
        XCTAssertTrue(newDrawing.waitForExistence(timeout: 10))

        newDrawing.tap()

        let back = app.buttons["back-to-gallery"]
        XCTAssertTrue(back.waitForExistence(timeout: 10), "the Canvas is shown")
        XCTAssertFalse(newDrawing.exists, "the Gallery is gone")

        back.tap()

        XCTAssertTrue(newDrawing.waitForExistence(timeout: 10), "back in the Gallery")
        let drawings = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'drawing-Untitled '"))
        XCTAssertEqual(drawings.count, 1, "the new Drawing is listed")
    }

    func testAListedDrawingOpensAgain() {
        app.buttons["new-drawing"].tap()
        app.buttons["back-to-gallery"].tap()
        let drawing = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'drawing-Untitled '")).firstMatch
        XCTAssertTrue(drawing.waitForExistence(timeout: 10))

        drawing.tap()

        XCTAssertTrue(app.buttons["back-to-gallery"].waitForExistence(timeout: 10))
    }

    /// The Gallery and menu buttons are the system's navigation bar buttons.
    func testTheCanvasHasNativeBackAndMenuButtonsInTheNavigationBar() {
        app.buttons["new-drawing"].tap()

        let bar = app.navigationBars.firstMatch
        XCTAssertTrue(bar.buttons["back-to-gallery"].waitForExistence(timeout: 10), "back is in the bar")
        XCTAssertTrue(bar.buttons["main-menu"].exists, "the menu is in the bar")
        XCTAssertLessThan(bar.buttons["back-to-gallery"].frame.midX, bar.buttons["main-menu"].frame.midX)
    }
}
