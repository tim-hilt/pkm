import Foundation
import Testing

@testable import WorkingTitle

/// Autosave of an open Drawing: saved a moment after the last change, and
/// at once when the Drawing is left or the app goes to the background.
/// Through the Drawing's editor and the `DrawingStore`'s files.
@MainActor @Suite struct AutosaveTests {
    private let directory: URL
    private let store: DrawingStore
    /// Short, so that the tests wait for it in real time.
    private static let delay = Duration.milliseconds(150)

    init() throws {
        directory = FileManager.default.temporaryDirectory
            .appending(path: "AutosaveTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        store = DrawingStore(directory: directory)
    }

    private func openNewDrawing() throws -> OpenDrawing {
        let name = try store.create(contents: Editor().save())
        return try OpenDrawing(name: name, store: store, autosaveDelay: Self.delay)
    }

    private func mouse(_ phase: PointerPhase, _ x: Double, _ y: Double) -> PointerEvent {
        PointerEvent(
            id: 0, phase: phase, x: x, y: y, kind: .mouse, pressure: 0.5, timestamp: 0,
            modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false))
    }

    /// Draws a rectangle and tells the Drawing its editor changed, as the
    /// Canvas does after input.
    private func drawRectangle(in drawing: OpenDrawing, to x: Double = 100) {
        drawing.editor.setTool(tool: .rectangle)
        drawing.editor.pointer(event: mouse(.down, 10, 10))
        drawing.editor.pointer(event: mouse(.move, x, 60))
        drawing.editor.pointer(event: mouse(.up, x, 60))
        drawing.editorDidChange()
    }

    private func savedElementCount(_ drawing: OpenDrawing) throws -> Int {
        let editor = Editor()
        try editor.load(bytes: store.open(drawing.name))
        return Int(editor.elementCount())
    }

    private func wait(_ duration: Duration) async throws { try await Task.sleep(for: duration) }

    // MARK: Open

    @Test func opensTheStoredDrawing() throws {
        let name = try store.create(contents: Data(contentsOf: SampleDrawing.url))

        let drawing = try OpenDrawing(name: name, store: store)

        #expect(drawing.name == name)
        #expect(drawing.editor.elementCount() == 8)
    }

    @Test func aDrawingThatCannotBeReadDoesNotOpen() throws {
        try store.save(Data("not json".utf8), as: "broken")

        #expect(throws: (any Error).self) { try OpenDrawing(name: "broken", store: store) }
    }

    // MARK: Debounce

    @Test func aChangeIsSavedAfterTheDelayNotBefore() async throws {
        let drawing = try openNewDrawing()

        drawRectangle(in: drawing)

        #expect(try savedElementCount(drawing) == 0)
        try await wait(Self.delay * 4)
        #expect(try savedElementCount(drawing) == 1)
    }

    @Test func eachChangeRestartsTheDelay() async throws {
        let drawing = try openNewDrawing()
        let before = try store.open(drawing.name)

        drawRectangle(in: drawing)
        try await wait(Self.delay * 2 / 3)
        drawRectangle(in: drawing, to: 200)
        try await wait(Self.delay * 2 / 3)

        // 4/3 of the delay since the first change, but only 2/3 since the last.
        #expect(try store.open(drawing.name) == before)
        try await wait(Self.delay * 4)
        #expect(try savedElementCount(drawing) == 2)
    }

    @Test func nothingIsSavedWithoutAChange() async throws {
        let drawing = try openNewDrawing()
        try store.save(Data("changed elsewhere".utf8), as: drawing.name)

        drawing.editorDidChange()
        try await wait(Self.delay * 4)

        #expect(try store.open(drawing.name) == Data("changed elsewhere".utf8))
    }

    // MARK: Background and close

    @Test func leavingTheDrawingSavesAtOnce() throws {
        let drawing = try openNewDrawing()
        drawRectangle(in: drawing)

        drawing.saveNow()

        #expect(try savedElementCount(drawing) == 1)
    }

    @Test func theDelayedSaveDoesNotRepeatAfterASaveAtOnce() async throws {
        let drawing = try openNewDrawing()
        drawRectangle(in: drawing)
        drawing.saveNow()
        let saved = try store.open(drawing.name)
        try store.save(Data("changed elsewhere".utf8), as: drawing.name)

        try await wait(Self.delay * 4)

        #expect(try store.open(drawing.name) == Data("changed elsewhere".utf8))
        #expect(saved != Data("changed elsewhere".utf8))
    }

    @Test func savingAtOnceWithoutChangesLeavesTheFileAlone() throws {
        let drawing = try openNewDrawing()
        try store.save(Data("changed elsewhere".utf8), as: drawing.name)

        drawing.saveNow()

        #expect(try store.open(drawing.name) == Data("changed elsewhere".utf8))
    }

    @Test func aSavedDrawingReopensAsItWas() throws {
        let drawing = try openNewDrawing()
        drawRectangle(in: drawing)
        drawing.saveNow()

        let reopened = try OpenDrawing(name: drawing.name, store: store)

        #expect(reopened.editor.elementCount() == 1)
        #expect(reopened.editor.save() == drawing.editor.save())
    }
}

/// Reopening a Drawing where it was left: its viewport is cached by the
/// `DrawingStore`, outside the `.excalidraw` file.
@MainActor @Suite struct ReopenViewportTests {
    private let directory: URL
    private let store: DrawingStore

    init() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "ReopenViewportTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        directory = root.appending(path: "Documents", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        store = DrawingStore(directory: directory, cacheDirectory: root.appending(path: "Caches"))
    }

    private func newDrawing() throws -> String { try store.create(contents: Editor().save()) }

    private func scrollAndZoom(_ drawing: OpenDrawing) {
        drawing.editor.setViewSize(width: 400, height: 300)
        drawing.editor.scroll(
            event: ScrollEvent(
                x: 0, y: 0, dx: 30, dy: 70, modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        drawing.editor.zoomIn()
        drawing.editor.zoomIn()
    }

    @Test func aNewDrawingOpensAtTheDefaultViewport() throws {
        let drawing = try OpenDrawing(name: newDrawing(), store: store)

        #expect(drawing.editor.viewport() == Editor().viewport())
    }

    @Test func aDrawingWithoutACachedViewportOpensAtTheDefaultViewport() throws {
        let name = try store.create(contents: Data(contentsOf: SampleDrawing.url))

        let drawing = try OpenDrawing(name: name, store: store)

        #expect(drawing.editor.viewport() == Editor().viewport())
    }

    @Test func leavingTheDrawingKeepsItsViewportForTheNextOpen() throws {
        let name = try newDrawing()
        let drawing = try OpenDrawing(name: name, store: store)
        scrollAndZoom(drawing)
        let left = drawing.editor.viewport()
        #expect(left != Editor().viewport())

        drawing.saveNow()
        let reopened = try OpenDrawing(name: name, store: store)

        #expect(reopened.editor.viewport() == left)
    }

    @Test func eachDrawingKeepsItsOwnViewport() throws {
        let (a, b) = try (newDrawing(), store.duplicate(newDrawing()))
        let first = try OpenDrawing(name: a, store: store)
        scrollAndZoom(first)
        first.saveNow()

        #expect(try OpenDrawing(name: b, store: store).editor.viewport() == Editor().viewport())
    }

    @Test func theViewportOfTheLastLeaveIsTheOneRestored() throws {
        let name = try newDrawing()
        let first = try OpenDrawing(name: name, store: store)
        scrollAndZoom(first)
        first.saveNow()
        let second = try OpenDrawing(name: name, store: store)
        second.editor.resetZoom()
        let left = second.editor.viewport()

        second.saveNow()

        #expect(try OpenDrawing(name: name, store: store).editor.viewport() == left)
    }

    @Test func aViewportIsKeptWithoutAnyChangeToTheScene() throws {
        let name = try newDrawing()
        let before = try store.list().first { $0.name == name }
        let drawing = try OpenDrawing(name: name, store: store)
        scrollAndZoom(drawing)

        drawing.saveNow()

        #expect(try store.list().first { $0.name == name } == before)
    }

    @Test func theViewportIsNotWrittenIntoTheFile() throws {
        let name = try newDrawing()
        let bytes = try store.open(name)
        let drawing = try OpenDrawing(name: name, store: store)
        scrollAndZoom(drawing)
        drawing.saveNow()

        #expect(try store.open(name) == bytes)
        #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path) == ["\(name).excalidraw"])

        // Not when the Scene is saved with it either.
        drawing.editor.setTool(tool: .rectangle)
        for (phase, x) in [(PointerPhase.down, 10.0), (.move, 90), (.up, 90)] {
            drawing.editor.pointer(
                event: PointerEvent(
                    id: 0, phase: phase, x: x, y: x, kind: .mouse, pressure: 0.5, timestamp: 0,
                    modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        }
        drawing.editorDidChange()
        drawing.saveNow()
        let saved = try String(decoding: store.open(name), as: UTF8.self)
        #expect(saved.contains("rectangle"))
        #expect(!saved.localizedCaseInsensitiveContains("zoom"))
        #expect(!saved.contains("scrollX"))
    }

    @Test func theCanvasShowsTheRestoredViewport() throws {
        let name = try newDrawing()
        let first = try OpenDrawing(name: name, store: store)
        scrollAndZoom(first)
        first.saveNow()
        let left = first.editor.viewport()

        let canvas = CanvasMetalView()
        let renderer = try #require(canvas.renderer)
        let reopened = try OpenDrawing(name: name, store: store)
        renderer.setGeometry(reopened.editor.geometry())
        canvas.editor = reopened.editor

        #expect(renderer.viewport == Viewport(left))
    }

    @Test func aViewportSurvivesRelaunchWhichIsAnotherStoreOnTheSameFolders() throws {
        let name = try newDrawing()
        let drawing = try OpenDrawing(name: name, store: store)
        scrollAndZoom(drawing)
        drawing.saveNow()
        let left = drawing.editor.viewport()

        let relaunched = DrawingStore(directory: store.directory, cacheDirectory: store.cacheDirectory)

        #expect(try OpenDrawing(name: name, store: relaunched).editor.viewport() == left)
    }
}

/// The Canvas tells when the Scene changed, which is what autosave listens to.
@MainActor @Suite struct SceneChangeTests {
    @Test func theCanvasReportsSceneChangesButNotNavigation() throws {
        let canvas = CanvasMetalView()
        let editor = Editor()
        _ = try #require(canvas.renderer)
        canvas.renderer?.setGeometry(editor.geometry())
        canvas.editor = editor
        var changes = 0
        canvas.onSceneChange = { changes += 1 }

        canvas.perform { $0.zoomIn() }
        #expect(changes == 0)

        canvas.perform { $0.setTool(tool: .rectangle) }
        for (phase, x) in [(PointerPhase.down, 10.0), (.move, 90), (.up, 90)] {
            canvas.send(
                PointerEvent(
                    id: 0, phase: phase, x: x, y: x, kind: .mouse, pressure: 0.5, timestamp: 0,
                    modifiers: Modifiers(shift: false, ctrl: false, alt: false, meta: false)))
        }
        #expect(changes > 0)
    }
}
