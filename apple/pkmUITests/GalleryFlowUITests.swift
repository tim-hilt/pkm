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

    /// Taps the Context's sidebar row: the sidebar is a column on an iPad and
    /// a Mac, and on an iPhone a list behind the navigation bar's back button.
    private func openSidebarRow(_ name: String) {
        let row = app.cells.staticTexts[name]
        if !row.waitForExistence(timeout: 5) {
            app.navigationBars.buttons["BackButton"].tap()
        }
        XCTAssertTrue(row.waitForExistence(timeout: 10), name)
        row.tap()
    }

    private func openDrawings() { openSidebarRow("Drawings") }

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
        XCTAssertFalse(app.cells.staticTexts["Tasks"].exists, "the sidebar is hidden")

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

    /// On an iPhone, the Gallery shown in place of another Context once missed its
    /// appearing, which left the cards without thumbnails.
    func testThumbnailsShowAfterComingFromAnotherContext() {
        openDrawings()
        app.buttons["new-drawing"].tap()
        drawARectangle()
        app.buttons["back-to-gallery"].tap()
        openSidebarRow("Tasks")
        XCTAssertTrue(app.buttons["add-filter"].waitForExistence(timeout: 10))  // no Favorites: the Filters list

        openDrawings()

        let card = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'drawing-Untitled '")).firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 10))
        let deadline = Date.now.addingTimeInterval(5)
        while !showsAThumbnail(card), Date.now < deadline { usleep(200_000) }
        XCTAssertTrue(showsAThumbnail(card), "the card shows the rectangle, not the empty frame")
    }

    /// Whether the top of the card, inside the thumbnail's frame, has more than one shade:
    /// the empty frame is a single fill.
    private func showsAThumbnail(_ card: XCUIElement) -> Bool {
        #if os(macOS)
            let image = card.screenshot().image.cgImage(forProposedRect: nil, context: nil, hints: nil)
        #else
            let image = card.screenshot().image.cgImage
        #endif
        guard let image else { return false }
        let width = image.width
        let height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = pixels.withUnsafeMutableBytes { buffer in
            CGContext(
                data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            )
            .map { $0.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height)) } != nil
        }
        guard drawn else { return false }
        var shades: (min: Int, max: Int) = (255, 0)
        for y in height / 10..<height / 2 {
            for x in width / 10..<width * 9 / 10 {
                let i = (y * width + x) * 4
                let shade = (Int(pixels[i]) + Int(pixels[i + 1]) + Int(pixels[i + 2])) / 3
                shades = (min(shades.min, shade), max(shades.max, shade))
            }
        }
        return shades.max - shades.min > 60
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

    func testSettingsIsASidebarRowOnPhonesAndPadsAndNoGearIsInAToolbar() throws {
        #if os(macOS)
            throw XCTSkip("a Mac has ⌘, and no Settings row")
        #endif
        for name in ["Tasks", "Notes", "Drawings", "Bookmarks", "Books"] {
            openSidebarRow(name)
            XCTAssertFalse(app.navigationBars.buttons["settings"].exists, name)
        }
        let back = app.navigationBars.buttons["BackButton"]
        if back.waitForExistence(timeout: 5) {  // an iPhone's list is one step back
            back.tap()
            XCTAssertTrue(back.waitForNonExistence(timeout: 5))  // a push during the pop is dropped
        }
        app.buttons["settings"].tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 5))
    }

    func testAPlaceholderSaysComingSoon() throws {
        openSidebarRow("Notes")
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

    /// Export, from the main menu: a preview of the image, to save or copy.
    func testExportShowsThePreviewToSaveOrCopy() {
        openDrawings()
        app.buttons["new-drawing"].tap()
        drawARectangle()

        app.navigationBars.firstMatch.buttons["main-menu"].tap()
        let export = app.buttons["Export…"]
        XCTAssertTrue(export.waitForExistence(timeout: 5))
        export.tap()

        XCTAssertTrue(app.staticTexts["Export Image"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.images["Preview"].exists)
        XCTAssertTrue(app.buttons["Save PNG…"].exists)
        let copy = app.buttons["Copy to Clipboard"]
        copy.tap()
        XCTAssertTrue(app.buttons["Copied"].waitForExistence(timeout: 5))
    }
}
