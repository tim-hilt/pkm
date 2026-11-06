import Foundation
import Testing

@testable import WorkingTitle

/// Seam 2: thumbnails are drawn by the Metal renderer offscreen, from a
/// Drawing's bytes.
@MainActor @Suite struct ThumbnailRendererTests {
    private let renderer: ThumbnailRenderer

    init() throws {
        renderer = try #require(ThumbnailRenderer())
    }

    /// A Drawing with one rectangle from `(0, 0)` to `(width, height)` in the Scene.
    private func rectangleDrawing(width: Double, height: Double) -> Data {
        let editor = Editor()
        editor.setTool(tool: .rectangle)
        let modifiers = Modifiers(shift: false, ctrl: false, alt: false, meta: false)
        for (phase, x, y) in [(PointerPhase.down, 0.0, 0.0), (.move, width, height), (.up, width, height)] {
            editor.pointer(
                event: PointerEvent(
                    id: 0, phase: phase, x: x, y: y, kind: .mouse, button: .primary, pressure: 0.5, timestamp: 0,
                    modifiers: modifiers))
        }
        return editor.save()
    }

    /// Whether any pixel of the columns `xs` and rows `ys` is darker than paper.
    private func hasInk(_ image: RenderedImage, columns xs: Range<Int>, rows ys: Range<Int>) -> Bool {
        ys.contains { y in xs.contains { x in image.red(x, y) < 200 } }
    }

    @Test func aThumbnailHasTheFixedSizeOfAGalleryCard() throws {
        let image = try #require(renderer.render(drawing: rectangleDrawing(width: 100, height: 80)))

        #expect(image.width == ThumbnailRenderer.width)
        #expect(image.height == ThumbnailRenderer.height)
        #expect(Double(image.width) / Double(image.height) == 4.0 / 3.0)
    }

    @Test func aThumbnailShowsTheDrawing() throws {
        let image = try #require(renderer.render(drawing: rectangleDrawing(width: 100, height: 80)))

        #expect(hasInk(image, columns: 0..<image.width, rows: 0..<image.height))
    }

    @Test func anEmptyDrawingIsBlankPaper() throws {
        let image = try #require(renderer.render(drawing: Editor().save()))

        #expect(!hasInk(image, columns: 0..<image.width, rows: 0..<image.height))
    }

    @Test func aLargeSceneIsShownWholeWithAMargin() throws {
        let image = try #require(renderer.render(drawing: rectangleDrawing(width: 3000, height: 1000)))
        let (width, height) = (image.width, image.height)
        let margin = 4

        #expect(hasInk(image, columns: 0..<width / 2, rows: 0..<height), "the left of the Scene is in view")
        #expect(hasInk(image, columns: width / 2..<width, rows: 0..<height), "the right of the Scene is in view")
        #expect(!hasInk(image, columns: 0..<margin, rows: 0..<height), "the frame keeps off the edge")
        #expect(!hasInk(image, columns: width - margin..<width, rows: 0..<height), "the frame keeps off the edge")
    }

    @Test func aSmallSceneIsNotBlownUp() throws {
        let image = try #require(renderer.render(drawing: rectangleDrawing(width: 20, height: 20)))

        #expect(!hasInk(image, columns: 0..<image.width / 4, rows: 0..<image.height), "drawn at 100 %, centred")
        #expect(hasInk(image, columns: image.width / 2 - 30..<image.width / 2 + 30, rows: 0..<image.height))
    }

    @Test func eachThumbnailShowsOnlyItsOwnDrawing() throws {
        _ = try #require(renderer.render(drawing: rectangleDrawing(width: 100, height: 80)))

        let image = try #require(renderer.render(drawing: Editor().save()))

        #expect(!hasInk(image, columns: 0..<image.width, rows: 0..<image.height))
        #expect(renderer.rendered == 2)
    }

    @Test func theSampleDrawingHasItsColours() throws {
        let image = try #require(renderer.render(drawing: Data(contentsOf: SampleDrawing.url)))

        #expect(hasInk(image, columns: 0..<image.width, rows: 0..<image.height))
    }

    @Test func aFileThatIsNoDrawingHasNoThumbnail() {
        #expect(renderer.render(drawing: Data("not json".utf8)) == nil)
        #expect(renderer.rendered == 0)
    }
}
