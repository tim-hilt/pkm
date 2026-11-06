import Foundation
import Testing

@testable import ExcalidrawNative

/// An open Drawing whose file is changed, renamed or deleted outside the app
/// (Files, Finder, a sync client), against a temporary folder.
@MainActor @Suite struct OutsideChangesTests {
    private let directory: URL
    private let store: DrawingStore

    init() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "OutsideChangesTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        directory = root.appending(path: "Documents", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        store = DrawingStore(directory: directory, cacheDirectory: root.appending(path: "Caches"))
    }

    private func openNewDrawing() throws -> OpenDrawing {
        let name = try store.create(contents: Editor().save())
        return try OpenDrawing(name: name, store: store, autosaveDelay: .seconds(60))
    }

    private func sample() throws -> Data { try Data(contentsOf: SampleDrawing.url) }

    private func file(_ name: String) -> URL { directory.appending(path: "\(name).excalidraw") }

    private func mouse(_ phase: PointerPhase, _ x: Double, _ y: Double) -> PointerEvent {
        PointerEvent(
            id: 0, phase: phase, x: x, y: y, kind: .mouse, button: .primary, pressure: 0.5, timestamp: 0,
            modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false))
    }

    private func drawRectangle(in drawing: OpenDrawing) {
        drawing.editor.setTool(tool: .rectangle)
        drawing.editor.pointer(event: mouse(.down, 10, 10))
        drawing.editor.pointer(event: mouse(.move, 100, 60))
        drawing.editor.pointer(event: mouse(.up, 100, 60))
        drawing.editorDidChange()
    }

    private func savedElementCount(_ name: String) throws -> Int {
        let editor = Editor()
        try editor.load(bytes: store.open(name))
        return Int(editor.elementCount())
    }

    // MARK: Changed

    @Test func anUntouchedFileIsLeftAlone() throws {
        let drawing = try openNewDrawing()

        #expect(drawing.syncWithFile() == .unchanged)
    }

    @Test func aChangeOutsideWithoutEditsIsMergedAndNoSaveOverwritesIt() throws {
        let drawing = try openNewDrawing()
        try store.save(sample(), as: drawing.name)

        #expect(drawing.syncWithFile() == .merged)
        #expect(drawing.editor.elementCount() == 8)
        drawing.saveNow()
        #expect(try savedElementCount(drawing.name) == 8)
        #expect(drawing.syncWithFile() == .unchanged)
    }

    @Test func aMergedChangeIsNoUndoStep() throws {
        let drawing = try openNewDrawing()
        drawRectangle(in: drawing)
        drawing.saveNow()
        try store.save(sample(), as: drawing.name)
        drawing.syncWithFile()

        drawing.editor.undo()

        // Undo took back our rectangle, not the arrival of the sample's.
        #expect(drawing.editor.elementCount() == 8)
        #expect(!drawing.editor.canUndo())
    }

    @Test func theViewportSurvivesAReload() throws {
        let drawing = try openNewDrawing()
        drawing.editor.setViewport(viewport: ViewportState(originX: 30, originY: 40, zoom: 2))
        try store.save(sample(), as: drawing.name)

        drawing.syncWithFile()

        #expect(drawing.editor.viewport() == ViewportState(originX: 30, originY: 40, zoom: 2))
    }

    @Test func aChangeOutsideWithUnsavedEditsIsMergedWithThemAndBothAreSaved() throws {
        let drawing = try openNewDrawing()
        drawRectangle(in: drawing)
        try store.save(sample(), as: drawing.name)

        #expect(drawing.syncWithFile() == .merged)
        drawing.saveNow()

        #expect(try savedElementCount(drawing.name) == 9)
        #expect(drawing.syncWithFile() == .unchanged)
    }

    // MARK: Deleted

    @Test func aDeletedFileIsNeverRecreated() throws {
        let drawing = try openNewDrawing()
        var deleted = 0
        drawing.onDeleted = { deleted += 1 }
        drawRectangle(in: drawing)
        try FileManager.default.removeItem(at: file(drawing.name))

        drawing.saveNow()
        drawing.close()

        #expect(!FileManager.default.fileExists(atPath: file(drawing.name).path))
        #expect(drawing.syncWithFile() == .deleted)
        #expect(deleted == 1)
    }

    @Test func aDeletedDrawingLeavesNoViewportCache() throws {
        let drawing = try openNewDrawing()
        try FileManager.default.removeItem(at: file(drawing.name))
        drawing.editor.setViewport(viewport: ViewportState(originX: 5, originY: 5, zoom: 3))

        drawing.saveNow()

        #expect(store.viewport(for: drawing.name) == nil)
    }

    @Test func aDeletedFileLeavesTheDrawingOpenWhenCloudBackingIsOnAndEditingBringsItBack() throws {
        let drawing = try openNewDrawing()
        drawing.keepsDeletedDrawingOpen = true
        var deleted = 0
        drawing.onDeleted = { deleted += 1 }
        try FileManager.default.removeItem(at: file(drawing.name))

        #expect(drawing.syncWithFile() == .removed)
        #expect(deleted == 0)
        drawRectangle(in: drawing)
        drawing.saveNow()

        #expect(try savedElementCount(drawing.name) == 1)
        #expect(drawing.syncWithFile() == .unchanged)
    }

    // MARK: Renamed

    @Test func aRenameBySyncIsFollowedAndTheNewFileMerged() throws {
        let drawing = try openNewDrawing()
        drawing.keepsDeletedDrawingOpen = true
        try store.save(sample(), as: drawing.name)
        drawing.syncWithFile()
        let old = drawing.name
        // Sync writes the file elsewhere under the new name and deletes the old.
        try FileManager.default.removeItem(at: file(old))
        try store.save(sample(), as: "Renamed")
        #expect(drawing.syncWithFile() == .removed)

        drawing.syncDidChange([old, "Renamed"])

        #expect(drawing.name == "Renamed")
        #expect(drawing.editor.elementCount() == 8)
        drawRectangle(in: drawing)
        drawing.saveNow()
        #expect(try savedElementCount("Renamed") == 9)
        #expect(!FileManager.default.fileExists(atPath: file(old).path))
    }

    @Test func aRenamedFileIsFollowedAndSavesGoToTheNewName() throws {
        let drawing = try openNewDrawing()
        let old = drawing.name
        try FileManager.default.moveItem(at: file(old), to: file("Renamed"))

        #expect(drawing.syncWithFile() == .renamed(to: "Renamed"))
        #expect(drawing.name == "Renamed")
        drawRectangle(in: drawing)
        drawing.saveNow()

        #expect(try savedElementCount("Renamed") == 1)
        #expect(!FileManager.default.fileExists(atPath: file(old).path))
    }

    @Test func aRenameFollowedWhileSavingSavesUnderTheNewName() throws {
        let drawing = try openNewDrawing()
        let old = drawing.name
        drawRectangle(in: drawing)
        try FileManager.default.moveItem(at: file(old), to: file("Renamed"))

        drawing.saveNow()

        #expect(drawing.name == "Renamed")
        #expect(try savedElementCount("Renamed") == 1)
        #expect(!FileManager.default.fileExists(atPath: file(old).path))
    }

    @Test func theCachesMoveWithARenamedDrawing() throws {
        let drawing = try openNewDrawing()
        let old = drawing.name
        let viewport = ViewportState(originX: 7, originY: 8, zoom: 2)
        drawing.editor.setViewport(viewport: viewport)
        drawing.saveNow()
        try FileManager.default.moveItem(at: file(old), to: file("Renamed"))

        drawing.syncWithFile()

        #expect(store.viewport(for: "Renamed") == viewport)
        #expect(store.viewport(for: old) == nil)
    }

    @Test func aFileReplacedByAnotherCountsAsChangedNotRenamed() throws {
        let drawing = try openNewDrawing()
        let old = drawing.name
        try FileManager.default.removeItem(at: file(old))
        try store.save(sample(), as: "Other")

        #expect(drawing.syncWithFile() == .deleted)
    }
}
