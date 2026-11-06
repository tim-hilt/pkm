import Foundation
import Testing

@testable import ExcalidrawNative

/// The Gallery's navigation: Gallery → Drawing → back, on a temporary folder.
@MainActor @Suite struct GalleryTests {
    private let store: DrawingStore

    init() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "GalleryTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        let directory = root.appending(path: "Documents", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        store = DrawingStore(directory: directory, cacheDirectory: root.appending(path: "Caches"))
    }

    @Test func startsWithTheStoredDrawingsAndNothingOpen() throws {
        try store.save(Editor().save(), as: "one")

        let gallery = Gallery(store: store)

        #expect(gallery.drawings.map(\.name) == ["one"])
        #expect(gallery.open == nil)
    }

    @Test func startingRemovesTheCachesOfDrawingsDeletedFromOutside() throws {
        let orphan = store.cacheDirectory(for: "deleted in Finder")
        try FileManager.default.createDirectory(at: orphan, withIntermediateDirectories: true)

        _ = Gallery(store: store)

        #expect(!FileManager.default.fileExists(atPath: orphan.path))
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
                    id: 0, phase: phase, x: x, y: x, kind: .mouse, button: .primary, pressure: 0.5, timestamp: 0,
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

    // MARK: Rename, duplicate, delete

    @Test func aRenamedDrawingIsListedUnderItsNewName() throws {
        try store.save(Editor().save(), as: "old")
        let gallery = Gallery(store: store)

        gallery.renameDrawing(named: "old", to: "new")

        #expect(gallery.drawings.map(\.name) == ["new"])
        #expect(gallery.failure == nil)
    }

    @Test func renamingToATakenNameTellsTheUserAndChangesNothing() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.save(Data("b".utf8), as: "b")
        let gallery = Gallery(store: store)

        gallery.renameDrawing(named: "a", to: "b")

        #expect(gallery.failure?.contains("“b”") == true)
        #expect(Set(gallery.drawings.map(\.name)) == ["a", "b"])
        #expect(try store.open("b") == Data("b".utf8))
    }

    @Test func renamingToAnEmptyNameTellsTheUser() throws {
        try store.save(Editor().save(), as: "a")
        let gallery = Gallery(store: store)

        gallery.renameDrawing(named: "a", to: " ")

        #expect(gallery.failure != nil)
        #expect(gallery.drawings.map(\.name) == ["a"])
    }

    @Test func aDuplicateIsListedNextToTheOriginal() throws {
        try store.save(Editor().save(), as: "plan")
        let gallery = Gallery(store: store)

        gallery.duplicateDrawing(named: "plan")

        #expect(gallery.drawings.count == 2)
        #expect(gallery.drawings.map(\.name).contains("plan"))
        #expect(gallery.open == nil, "the Gallery stays")
        #expect(gallery.failure == nil)
    }

    @Test func aDeletedDrawingIsGone() throws {
        try store.save(Editor().save(), as: "a")
        try store.save(Editor().save(), as: "b")
        let gallery = Gallery(store: store)

        gallery.deleteDrawing(named: "a")

        #expect(gallery.drawings.map(\.name) == ["b"])
        #expect(throws: (any Error).self) { try store.open("a") }
    }

    @Test func aFailedChangeIsClearedByTheNextSuccessfulOne() throws {
        try store.save(Editor().save(), as: "a")
        let gallery = Gallery(store: store)
        gallery.deleteDrawing(named: "missing")
        #expect(gallery.failure != nil)

        gallery.duplicateDrawing(named: "a")

        #expect(gallery.failure == nil)
    }

    // MARK: Files added, renamed and removed outside the app

    private func folder(_ name: String) -> URL { store.directory.appending(path: "\(name).excalidraw") }

    /// Follows the folder while `body` runs, as the Gallery's view does while it is shown.
    private func following(_ gallery: Gallery, _ body: () async throws -> Void) async rethrows {
        let following = Task { await gallery.followChangesFromOutside() }
        defer { following.cancel() }
        await Task.yield()  // lets it start watching
        try await body()
    }

    @Test func aFileAddedFromOutsideAppearsWithoutRestarting() async throws {
        let gallery = Gallery(store: store)

        try await following(gallery) {
            try Editor().save().write(to: folder("from Files"))

            #expect(await eventually { gallery.drawings.map(\.name) == ["from Files"] })
        }
    }

    @Test func aFileRemovedFromOutsideDisappears() async throws {
        try store.save(Editor().save(), as: "a")
        try store.save(Editor().save(), as: "b")
        let gallery = Gallery(store: store)

        try await following(gallery) {
            try FileManager.default.removeItem(at: folder("a"))

            #expect(await eventually { gallery.drawings.map(\.name) == ["b"] })
        }
    }

    @Test func aFileRenamedFromOutsideIsListedUnderItsNewName() async throws {
        try store.save(Editor().save(), as: "old")
        let gallery = Gallery(store: store)

        try await following(gallery) {
            try FileManager.default.moveItem(at: folder("old"), to: folder("new"))

            #expect(await eventually { gallery.drawings.map(\.name) == ["new"] })
        }
    }

    @Test func aMalformedFileFromOutsideIsListedAndNeverOverwritten() async throws {
        let junk = Data("not json".utf8)
        let gallery = Gallery(store: store)

        try await following(gallery) {
            try junk.write(to: folder("broken"))
            #expect(await eventually { gallery.drawings.map(\.name) == ["broken"] })
        }

        gallery.openDrawing(named: "broken")
        gallery.close()
        #expect(gallery.failure != nil)
        gallery.duplicateDrawing(named: "broken")
        gallery.createDrawing()
        gallery.close()
        #expect(try store.open("broken") == junk)
    }

    // MARK: Export

    @Test func anExportedDrawingIsItsFileWithTheSameBytes() throws {
        let bytes = try Data(contentsOf: SampleDrawing.url)
        try store.save(bytes, as: "Flowchart")
        let gallery = Gallery(store: store)

        let url = try #require(gallery.exportFile(named: "Flowchart"))

        #expect(url.lastPathComponent == "Flowchart.excalidraw")
        #expect(try Data(contentsOf: url) == bytes)
    }

    @Test func exportingTheOpenDrawingIncludesChangesNotYetSaved() throws {
        let gallery = Gallery(store: store)
        gallery.createDrawing()
        let drawing = try #require(gallery.open)
        drawing.editor.setTool(tool: .rectangle)
        for (phase, x) in [(PointerPhase.down, 10.0), (.move, 90), (.up, 90)] {
            drawing.editor.pointer(
                event: PointerEvent(
                    id: 0, phase: phase, x: x, y: x, kind: .mouse, button: .primary, pressure: 0.5, timestamp: 0,
                    modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        }

        let url = try #require(gallery.exportFile(named: drawing.name))

        let exported = Editor()
        try exported.load(bytes: Data(contentsOf: url))
        #expect(exported.elementCount() == 1)
    }

    @Test func exportingAMissingDrawingTellsTheUser() {
        let gallery = Gallery(store: store)

        #expect(gallery.exportFile(named: "Gone") == nil)
        #expect(gallery.failure != nil)
    }
}
