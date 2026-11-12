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

    /// The app opens on Tasks; Drawings is a tab, or a sidebar row on an iPad
    /// and a Mac.
    private func openDrawings() {
        let tab = app.tabBars.buttons["Drawings"]
        let row = app.buttons["Drawings"]
        if tab.waitForExistence(timeout: 10) { tab.tap() } else if row.waitForExistence(timeout: 10) { row.tap() }
    }

    /// An empty Drawing is not kept when left, so these tests draw something.
    private func drawARectangle() {
        let rectangle = app.buttons["Rectangle"]
        XCTAssertTrue(rectangle.waitForExistence(timeout: 10))
        rectangle.tap()
        let window = app.windows.firstMatch
        window.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5)).press(
            forDuration: 0.1, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.6, dy: 0.6)))
    }

    func testCreatingADrawingOpensTheCanvasAndBackReturnsToTheGalleryWithIt() {
        openDrawings()
        let newDrawing = app.buttons["new-drawing"]
        XCTAssertTrue(newDrawing.waitForExistence(timeout: 10))

        newDrawing.tap()

        let back = app.buttons["back-to-gallery"]
        XCTAssertTrue(back.waitForExistence(timeout: 10), "the Canvas is shown")
        XCTAssertFalse(newDrawing.exists, "the Gallery is gone")
        XCTAssertFalse(app.tabBars.firstMatch.exists, "the tab bar is hidden")

        drawARectangle()
        back.tap()

        XCTAssertTrue(newDrawing.waitForExistence(timeout: 10), "back in the Gallery")
        let drawings = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'drawing-Untitled '"))
        XCTAssertEqual(drawings.count, 1, "the new Drawing is listed")
    }

    func testAListedDrawingOpensAgain() {
        openDrawings()
        app.buttons["new-drawing"].tap()
        drawARectangle()
        app.buttons["back-to-gallery"].tap()
        let drawing = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'drawing-Untitled '")).firstMatch
        XCTAssertTrue(drawing.waitForExistence(timeout: 10))

        drawing.tap()

        XCTAssertTrue(app.buttons["back-to-gallery"].waitForExistence(timeout: 10))
    }

    /// The confirmation is a lone Delete button next to the card, like Photos'.
    func testDeletingAsksWithALoneDeleteButtonAtTheCard() {
        openDrawings()
        app.buttons["new-drawing"].tap()
        drawARectangle()
        app.buttons["back-to-gallery"].tap()
        let card = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'drawing-Untitled '")).firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 10))

        card.press(forDuration: 1)
        app.buttons["delete"].tap()

        let confirm = app.buttons["delete-confirm"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "delete-confirmation"
        shot.lifetime = .keepAlways
        add(shot)
        XCTAssertTrue(
            (card.frame.minX...card.frame.maxX).contains(confirm.frame.midX), "the button is at the card")
        let texts = app.staticTexts.matching(NSPredicate(format: "label CONTAINS[c] 'delete'"))
        XCTAssertEqual(texts.count, 0, "no title or message")

        confirm.tap()

        XCTAssertTrue(card.waitForNonExistence(timeout: 5), "the Drawing is gone")
    }

    func testEveryContextsRootHasTheSettingsGearOnPhonesAndPads() throws {
        let tabs = app.tabBars.firstMatch
        try XCTSkipUnless(tabs.waitForExistence(timeout: 10), "a Mac has a sidebar and ⌘,")
        for name in ["Tasks", "Notes", "Drawings", "Bookmarks", "Books"] {
            tabs.buttons[name].tap()
            XCTAssertTrue(app.navigationBars.buttons["settings"].waitForExistence(timeout: 5), name)
        }
    }

    func testAPlaceholderSaysComingSoon() throws {
        XCTAssertTrue(app.staticTexts["Coming soon"].waitForExistence(timeout: 10))
    }

    /// The Gallery and menu buttons are the system's navigation bar buttons.
    func testTheCanvasHasNativeBackAndMenuButtonsInTheNavigationBar() {
        openDrawings()
        app.buttons["new-drawing"].tap()

        let bar = app.navigationBars.firstMatch
        XCTAssertTrue(bar.buttons["back-to-gallery"].waitForExistence(timeout: 10), "back is in the bar")
        XCTAssertTrue(bar.buttons["main-menu"].exists, "the menu is in the bar")
        XCTAssertLessThan(bar.buttons["back-to-gallery"].frame.midX, bar.buttons["main-menu"].frame.midX)
    }
}
