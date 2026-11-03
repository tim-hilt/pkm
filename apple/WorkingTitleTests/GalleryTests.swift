import Foundation
import Testing

@testable import WorkingTitle

/// The Gallery's navigation: Gallery → Drawing → back, on a temporary folder.
@MainActor @Suite struct GalleryTests {
    private let store: DrawingStore

    init() throws {
        let directory = FileManager.default.temporaryDirectory
            .appending(path: "GalleryTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        store = DrawingStore(directory: directory)
    }

    @Test func startsWithTheStoredDrawingsAndNothingOpen() throws {
        try store.save(Editor().save(), as: "one")

        let gallery = Gallery(store: store)

        #expect(gallery.drawings.map(\.name) == ["one"])
        #expect(gallery.open == nil)
    }

    @Test func creatingADrawingOpensAnEmptyOne() {
        let gallery = Gallery(store: store)

        gallery.createDrawing()

        #expect(gallery.open?.name.hasPrefix("Untitled ") == true)
        #expect(gallery.open?.editor.elementCount() == 0)
        #expect(gallery.failure == nil)
    }

    @Test func goingBackSavesTheDrawingAndListsIt() throws {
        let gallery = Gallery(store: store)
        gallery.createDrawing()
        let drawing = try #require(gallery.open)
        drawing.editor.setTool(tool: .rectangle)
        for (phase, x) in [(PointerPhase.down, 10.0), (.move, 90), (.up, 90)] {
            drawing.editor.pointer(
                event: PointerEvent(
                    id: 0, phase: phase, x: x, y: x, kind: .mouse, pressure: 0.5, timestamp: 0,
                    modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        }
        drawing.editorDidChange()

        gallery.close()

        #expect(gallery.open == nil)
        #expect(gallery.drawings.map(\.name) == [drawing.name])
        let saved = Editor()
        try saved.load(bytes: store.open(drawing.name))
        #expect(saved.elementCount() == 1)
    }

    @Test func openingAListedDrawingShowsIt() throws {
        try store.save(Data(contentsOf: SampleDrawing.url), as: "sample")
        let gallery = Gallery(store: store)

        gallery.openDrawing(named: "sample")

        #expect(gallery.open?.name == "sample")
        #expect(gallery.open?.editor.elementCount() == 8)
    }

    @Test func aDrawingThatCannotBeOpenedStaysInTheGalleryWithAFailure() throws {
        try store.save(Data("not json".utf8), as: "broken")
        let gallery = Gallery(store: store)

        gallery.openDrawing(named: "broken")

        #expect(gallery.open == nil)
        #expect(gallery.failure != nil)
        #expect(try store.open("broken") == Data("not json".utf8))
    }
}
