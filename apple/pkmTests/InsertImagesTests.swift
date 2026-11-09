import Foundation
import Testing

@testable import pkm

/// Pasting and dropping images through the Canvas: the pictures are prepared
/// for the core (downscaled), inserted as Elements and shown by the renderer.
/// The platform's pasteboard and drag sessions only deliver bytes to
/// `insertImages`, which is where these tests start.
@MainActor @Suite struct InsertImagesTests {
    @MainActor private struct Setup {
        let canvas = CanvasMetalView()
        let editor = Editor()
        let controller = CanvasController(settings: .isolated())
    }

    private func makeSetup() throws -> Setup {
        let setup = Setup()
        _ = try #require(setup.canvas.renderer)
        setup.editor.setViewSize(width: 800, height: 600)
        setup.canvas.editor = setup.editor
        setup.controller.canvas = setup.canvas
        return setup
    }

    @Test func aPastedImageBecomesTheSelectedImageElement() async throws {
        let setup = try makeSetup()

        await setup.canvas.insertImages(from: [TestImages.quadrants(size: 40)])

        #expect(setup.editor.elementCount() == 1)
        #expect(setup.editor.selection().count == 1)
        let geometry = setup.editor.geometry()
        #expect(geometry.imageCount() == 1)
        #expect(setup.controller.canUndo)
    }

    @Test func aDroppedImageIsCentredOnTheDropPoint() async throws {
        let setup = try makeSetup()

        await setup.canvas.insertImages(from: [TestImages.quadrants(size: 40)], at: CGPoint(x: 100, y: 80))

        let saved = String(decoding: setup.editor.save(), as: UTF8.self)
        #expect(saved.contains(#""x": 80"#) && saved.contains(#""y": 60"#), "\(saved)")
    }

    @Test func aLargeImageIsStoredDownscaledToTheLimit() async throws {
        let setup = try makeSetup()
        let wide = TestImages.png(width: 3000, height: 1000) { _, _ in TestImages.red }

        await setup.canvas.insertImages(from: [wide])

        let element = String(decoding: setup.editor.save(), as: UTF8.self)
        let id = try #require(
            element.range(of: #""fileId": ""#).map { range in
                element[range.upperBound...].prefix { $0 != "\"" }
            })
        let stored = try #require(setup.editor.fileBytes(id: String(id)))
        let decoded = try #require(ImageDecoder.decode(stored))
        #expect((decoded.width, decoded.height) == (1440, 480))
    }

    @Test func insertingIsUndoable() async throws {
        let setup = try makeSetup()
        await setup.canvas.insertImages(from: [TestImages.quadrants(size: 40)])

        setup.controller.undo()

        #expect(setup.editor.elementCount() == 0)
    }

    @Test func whatIsNoImageIsSkipped() async throws {
        let setup = try makeSetup()

        await setup.canvas.insertImages(from: [Data("hello".utf8)])

        #expect(setup.editor.elementCount() == 0)
        #expect(!setup.controller.canUndo)
    }
}
