import Foundation
import Testing

@testable import ExcalidrawNative

/// Seam 3: the `DrawingStore` against a temporary directory. Drawings are
/// `<name>.excalidraw` files in it; the tests look at the directory itself to
/// see what a user would see in Files or Finder.
@Suite struct DrawingStoreTests {
    private let directory: URL
    private let cacheDirectory: URL
    private let store: DrawingStore

    init() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "DrawingStoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        directory = root.appending(path: "Documents", directoryHint: .isDirectory)
        cacheDirectory = root.appending(path: "Caches", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        store = DrawingStore(directory: directory, cacheDirectory: cacheDirectory)
    }

    /// Puts a cache file (a thumbnail, say) next to the Drawing the way the
    /// caches of later features will.
    private func cache(_ name: String, file: String = "thumbnail.png") throws {
        let folder = store.cacheDirectory(for: name)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try Data("cached".utf8).write(to: folder.appending(path: file))
    }

    private func hasCache(_ name: String, file: String = "thumbnail.png") -> Bool {
        FileManager.default.fileExists(atPath: store.cacheDirectory(for: name).appending(path: file).path)
    }

    private func fileNames() throws -> [String] {
        try FileManager.default.contentsOfDirectory(atPath: directory.path).sorted()
    }

    private func setModified(_ name: String, _ date: Date) throws {
        try FileManager.default.setAttributes(
            [.modificationDate: date],
            ofItemAtPath: directory.appending(path: "\(name).excalidraw").path)
    }

    private static let utc = TimeZone(identifier: "UTC")!

    // MARK: List

    @Test func anEmptyFolderListsNoDrawings() throws {
        #expect(try store.list().isEmpty)
    }

    @Test func drawingsAreListedByLastModifiedNewestFirst() throws {
        for name in ["old", "new", "middle"] { try store.save(Data("{}".utf8), as: name) }
        try setModified("old", Date(timeIntervalSince1970: 1000))
        try setModified("middle", Date(timeIntervalSince1970: 2000))
        try setModified("new", Date(timeIntervalSince1970: 3000))

        let listed = try store.list()

        #expect(listed.map(\.name) == ["new", "middle", "old"])
        #expect(listed.map(\.modified) == [3000, 2000, 1000].map(Date.init(timeIntervalSince1970:)))
    }

    @Test func savingMovesADrawingToTheTop() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.save(Data("b".utf8), as: "b")
        try setModified("a", Date(timeIntervalSince1970: 1000))
        try setModified("b", Date(timeIntervalSince1970: 2000))

        try store.save(Data("a2".utf8), as: "a")

        #expect(try store.list().map(\.name) == ["a", "b"])
    }

    @Test func onlyExcalidrawFilesAreDrawings() throws {
        try store.save(Data("{}".utf8), as: "drawing")
        try Data().write(to: directory.appending(path: "notes.txt"))
        try Data().write(to: directory.appending(path: ".hidden.excalidraw"))
        try FileManager.default.createDirectory(
            at: directory.appending(path: "folder.excalidraw"), withIntermediateDirectories: false)

        #expect(try store.list().map(\.name) == ["drawing"])
    }

    // MARK: Create

    @Test func aNewDrawingIsNamedUntitledAndItsDateAndTime() throws {
        // 2026-11-03 19:27:31 UTC
        let date = Date(timeIntervalSince1970: 1_793_734_051)

        let name = try store.create(at: date, in: Self.utc, contents: Data("{}".utf8))

        #expect(name == "Untitled 2026-11-03 19.27.31")
        #expect(try fileNames() == ["Untitled 2026-11-03 19.27.31.excalidraw"])
    }

    @Test func aNewDrawingStartsWithTheGivenContents() throws {
        let name = try store.create(at: .now, in: Self.utc, contents: Data("start".utf8))

        #expect(try store.open(name) == Data("start".utf8))
    }

    @Test func drawingsCreatedAtTheSameTimeGetDistinctNames() throws {
        let date = Date(timeIntervalSince1970: 1_793_734_051)

        let first = try store.create(at: date, in: Self.utc, contents: Data("1".utf8))
        let second = try store.create(at: date, in: Self.utc, contents: Data("2".utf8))

        #expect(first != second)
        #expect(try store.open(first) == Data("1".utf8))
        #expect(try store.open(second) == Data("2".utf8))
    }

    // MARK: Adding

    @Test func anAddedDrawingKeepsItsNameAndContents() throws {
        let name = try store.add(Data("x".utf8), named: "Flowchart")

        #expect(name == "Flowchart")
        #expect(try store.open("Flowchart") == Data("x".utf8))
    }

    @Test func addingUnderATakenNameGetsADistinctNameAndKeepsTheOther() throws {
        try store.save(Data("old".utf8), as: "Flowchart")

        let second = try store.add(Data("new".utf8), named: "Flowchart")
        let third = try store.add(Data("newer".utf8), named: "Flowchart")

        #expect(second == "Flowchart 2")
        #expect(third == "Flowchart 3")
        #expect(try store.open("Flowchart") == Data("old".utf8))
        #expect(try store.open("Flowchart 2") == Data("new".utf8))
    }

    @Test func aSavedDrawingOpensWithTheSameBytes() throws {
        let bytes = Data("{\"type\":\"excalidraw\"}".utf8)

        try store.save(bytes, as: "sketch")

        #expect(try store.open("sketch") == bytes)
        #expect(try fileNames() == ["sketch.excalidraw"])
    }

    @Test func savingReplacesTheEarlierContents() throws {
        try store.save(Data("first".utf8), as: "sketch")
        try store.save(Data("second".utf8), as: "sketch")

        #expect(try store.open("sketch") == Data("second".utf8))
    }

    @Test func openingAMissingDrawingFails() {
        #expect(throws: (any Error).self) { try store.open("missing") }
    }

    @Test func namesThatLeaveTheFolderAreRejected() throws {
        #expect(throws: DrawingStore.InvalidName.self) { try store.save(Data(), as: "../escape") }
        #expect(throws: DrawingStore.InvalidName.self) { try store.open("a/b") }
        #expect(throws: DrawingStore.InvalidName.self) { try store.save(Data(), as: "") }
    }

    // MARK: Atomic saves

    @Test func aSaveLeavesNothingButTheDrawingBehind() throws {
        for round in 0..<20 { try store.save(Data(String(repeating: "x", count: round * 1000).utf8), as: "sketch") }

        #expect(try fileNames() == ["sketch.excalidraw"])
    }

    @Test func aSaveThatFailsLeavesTheEarlierContentsIntact() throws {
        try store.save(Data("precious".utf8), as: "sketch")
        // A read-only folder cannot take the new file, so the save fails
        // part-way: the old Drawing must still be whole.
        try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: directory.path)
        defer {
            try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: directory.path)
        }

        #expect(throws: (any Error).self) { try store.save(Data("lost".utf8), as: "sketch") }

        #expect(try store.open("sketch") == Data("precious".utf8))
        #expect(try fileNames() == ["sketch.excalidraw"])
    }

    // MARK: Rename

    @Test func renamingChangesTheFileNameAndKeepsTheContents() throws {
        try store.save(Data("sketch".utf8), as: "old")

        let name = try store.rename("old", to: "new")

        #expect(name == "new")
        #expect(try fileNames() == ["new.excalidraw"])
        #expect(try store.open("new") == Data("sketch".utf8))
    }

    @Test func renamingKeepsTheCachesOfTheDrawing() throws {
        try store.save(Data("sketch".utf8), as: "old")
        try cache("old")

        _ = try store.rename("old", to: "new")

        #expect(hasCache("new"))
        #expect(!hasCache("old"))
    }

    @Test func renamingToATakenNameFailsAndChangesNothing() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.save(Data("b".utf8), as: "b")

        #expect(throws: DrawingStore.NameTaken(name: "b")) { try store.rename("a", to: "b") }

        #expect(try store.open("a") == Data("a".utf8))
        #expect(try store.open("b") == Data("b".utf8))
    }

    @Test func renamingToTheSameNameChangesNothing() throws {
        try store.save(Data("a".utf8), as: "a")

        #expect(try store.rename("a", to: "a") == "a")

        #expect(try fileNames() == ["a.excalidraw"])
    }

    @Test func renamingOnlyTheCaseKeepsTheDrawing() throws {
        try store.save(Data("a".utf8), as: "sketch")

        #expect(try store.rename("sketch", to: "Sketch") == "Sketch")

        #expect(try fileNames() == ["Sketch.excalidraw"])
        #expect(try store.open("Sketch") == Data("a".utf8))
    }

    @Test func renamingTrimsSurroundingWhitespace() throws {
        try store.save(Data("a".utf8), as: "a")

        #expect(try store.rename("a", to: "  spaced \n") == "spaced")

        #expect(try fileNames() == ["spaced.excalidraw"])
    }

    @Test func renamingToANameThatIsNotUsableFails() throws {
        try store.save(Data("a".utf8), as: "a")

        for name in ["", "   ", "a/b", "..", ".hidden"] {
            #expect(throws: DrawingStore.InvalidName.self) { try store.rename("a", to: name) }
        }
        #expect(try fileNames() == ["a.excalidraw"])
    }

    @Test func renamingAMissingDrawingFails() {
        #expect(throws: (any Error).self) { try store.rename("missing", to: "new") }
    }

    // MARK: Duplicate

    @Test func aDuplicateIsACopyUnderADistinctName() throws {
        try store.save(Data("sketch".utf8), as: "plan")

        let copy = try store.duplicate("plan")

        #expect(copy != "plan")
        #expect(copy.hasPrefix("plan"))
        #expect(try store.open(copy) == Data("sketch".utf8))
        #expect(try store.open("plan") == Data("sketch".utf8))
        #expect(try fileNames().count == 2)
    }

    @Test func duplicatesOfTheSameDrawingGetDistinctNames() throws {
        try store.save(Data("sketch".utf8), as: "plan")

        let names = try (0..<3).map { _ in try store.duplicate("plan") }

        #expect(Set(names + ["plan"]).count == 4)
        #expect(try fileNames().count == 4)
    }

    @Test func aDuplicateIsTheNewestDrawing() throws {
        try store.save(Data("a".utf8), as: "plan")
        try setModified("plan", Date(timeIntervalSince1970: 1000))

        let copy = try store.duplicate("plan")

        #expect(try store.list().map(\.name) == [copy, "plan"])
    }

    @Test func duplicatingAMissingDrawingFails() {
        #expect(throws: (any Error).self) { try store.duplicate("missing") }
    }

    // MARK: Delete

    @Test func deletingRemovesTheFile() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.save(Data("b".utf8), as: "b")

        try store.delete("a")

        #expect(try fileNames() == ["b.excalidraw"])
        #expect(try store.list().map(\.name) == ["b"])
    }

    @Test func deletingRemovesTheCachesOfTheDrawingOnly() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.save(Data("b".utf8), as: "b")
        try cache("a")
        try cache("a", file: "viewport.json")
        try cache("b")

        try store.delete("a")

        #expect(!hasCache("a"))
        #expect(!hasCache("a", file: "viewport.json"))
        #expect(hasCache("b"))
    }

    @Test func deletingADrawingWithoutCachesWorks() throws {
        try store.save(Data("a".utf8), as: "a")

        try store.delete("a")

        #expect(try fileNames().isEmpty)
    }

    @Test func aDeletedNameCanBeUsedAgainWithoutOldCaches() throws {
        try store.save(Data("a".utf8), as: "a")
        try cache("a")
        try store.delete("a")

        try store.save(Data("new".utf8), as: "a")

        #expect(!hasCache("a"))
    }

    @Test func deletingAMissingDrawingFails() {
        #expect(throws: (any Error).self) { try store.delete("missing") }
    }

    @Test func aDuplicateDoesNotShareTheCachesOfTheOriginal() throws {
        try store.save(Data("a".utf8), as: "a")
        try cache("a")

        let copy = try store.duplicate("a")
        try store.delete("a")

        #expect(!hasCache(copy), "caches are made again for the copy, never borrowed")
    }

    // MARK: Caches of Drawings deleted from outside

    private func deleteFromOutside(_ name: String) throws {
        try FileManager.default.removeItem(at: directory.appending(path: "\(name).excalidraw"))
    }

    @Test func aNewDrawingUnderTheNameOfOneDeletedFromOutsideStartsWithoutItsCaches() throws {
        try store.save(Data("old".utf8), as: "Flowchart")
        try cache("Flowchart", file: "viewport.json")
        try deleteFromOutside("Flowchart")

        let name = try store.add(Data("new".utf8), named: "Flowchart")

        #expect(name == "Flowchart")
        #expect(!hasCache("Flowchart", file: "viewport.json"))
    }

    @Test func renamingOntoTheNameOfOneDeletedFromOutsideDropsItsCaches() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.save(Data("b".utf8), as: "b")
        try cache("b", file: "viewport.json")
        try deleteFromOutside("b")

        _ = try store.rename("a", to: "b")

        #expect(!hasCache("b", file: "viewport.json"))
    }

    @Test func orphanedCachesAreRemovedAndOthersKept() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.save(Data("b".utf8), as: "b")
        try cache("a")
        try cache("b")
        try deleteFromOutside("b")

        store.removeOrphanedCaches()

        #expect(hasCache("a"))
        #expect(!hasCache("b"))
        #expect(try fileNames() == ["a.excalidraw"])
    }

    @Test func removingOrphanedCachesWithoutAnyCachesDoesNothing() throws {
        try store.save(Data("a".utf8), as: "a")

        store.removeOrphanedCaches()

        #expect(try fileNames() == ["a.excalidraw"])
    }

    // MARK: Thumbnail cache

    private func entry(_ name: String) throws -> DrawingStore.Entry {
        try #require(store.list().first { $0.name == name })
    }

    @Test func aDrawingWithoutAThumbnailHasNoCachedOne() throws {
        try store.save(Data("a".utf8), as: "a")

        #expect(try store.thumbnail(for: entry("a")) == nil)
    }

    @Test func aSavedThumbnailIsReturnedWhileTheDrawingIsUnchanged() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.saveThumbnail(Data("png".utf8), for: entry("a"))

        #expect(try store.thumbnail(for: entry("a")) == Data("png".utf8))
    }

    @Test func aThumbnailIsStaleOnceTheDrawingChanged() throws {
        try store.save(Data("a".utf8), as: "a")
        try setModified("a", Date(timeIntervalSince1970: 1000))
        try store.saveThumbnail(Data("png".utf8), for: entry("a"))

        try store.save(Data("changed".utf8), as: "a")
        try setModified("a", Date(timeIntervalSince1970: 2000))

        #expect(try store.thumbnail(for: entry("a")) == nil)
    }

    @Test func aThumbnailIsStaleWhenTheDrawingWasReplacedByAnOlderFile() throws {
        try store.save(Data("a".utf8), as: "a")
        try setModified("a", Date(timeIntervalSince1970: 2000))
        try store.saveThumbnail(Data("png".utf8), for: entry("a"))

        try setModified("a", Date(timeIntervalSince1970: 1000))

        #expect(try store.thumbnail(for: entry("a")) == nil)
    }

    @Test func aNewThumbnailReplacesTheOldOne() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.saveThumbnail(Data("one".utf8), for: entry("a"))
        try store.saveThumbnail(Data("two".utf8), for: entry("a"))

        #expect(try store.thumbnail(for: entry("a")) == Data("two".utf8))
    }

    @Test func aThumbnailLivesOutsideTheDrawingsFolderAndLeavesTheFileAlone() throws {
        try store.save(Data("a".utf8), as: "a")
        let before = try entry("a")

        try store.saveThumbnail(Data("png".utf8), for: before)

        #expect(try fileNames() == ["a.excalidraw"])
        #expect(try store.open("a") == Data("a".utf8))
        #expect(try entry("a") == before, "saving a thumbnail is not a change of the Drawing")
        #expect(hasCache("a"))
    }

    @Test func aThumbnailFollowsItsRenamedDrawing() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.saveThumbnail(Data("png".utf8), for: entry("a"))

        _ = try store.rename("a", to: "b")

        #expect(try store.thumbnail(for: entry("b")) == Data("png".utf8))
    }

    @Test func aDuplicateStartsWithoutAThumbnail() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.saveThumbnail(Data("png".utf8), for: entry("a"))

        let copy = try store.duplicate("a")

        #expect(try store.thumbnail(for: entry(copy)) == nil)
    }

    // MARK: Viewport cache

    private let viewport = ViewportState(originX: -120.5, originY: 40, zoom: 2.5)

    @Test func aDrawingWithoutACachedViewportHasNone() throws {
        try store.save(Data("a".utf8), as: "a")

        #expect(store.viewport(for: "a") == nil)
    }

    @Test func aCachedViewportComesBackAsItWas() throws {
        try store.save(Data("a".utf8), as: "a")

        try store.saveViewport(viewport, for: "a")

        #expect(store.viewport(for: "a") == viewport)
    }

    @Test func theLastCachedViewportWins() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.saveViewport(viewport, for: "a")

        try store.saveViewport(ViewportState(originX: 1, originY: 2, zoom: 0.5), for: "a")

        #expect(store.viewport(for: "a") == ViewportState(originX: 1, originY: 2, zoom: 0.5))
    }

    @Test func aViewportThatCannotBeReadIsNone() throws {
        try store.save(Data("a".utf8), as: "a")
        try cache("a", file: "viewport.json")

        #expect(store.viewport(for: "a") == nil)
    }

    @Test func aViewportLivesOutsideTheDrawingsFolderAndLeavesTheFileAlone() throws {
        try store.save(Data("a".utf8), as: "a")
        let before = try entry("a")

        try store.saveViewport(viewport, for: "a")

        #expect(try fileNames() == ["a.excalidraw"])
        #expect(try store.open("a") == Data("a".utf8))
        #expect(try entry("a") == before, "saving a viewport is not a change of the Drawing")
    }

    @Test func aViewportAndAThumbnailDoNotGetInEachOthersWay() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.saveThumbnail(Data("png".utf8), for: entry("a"))

        try store.saveViewport(viewport, for: "a")

        #expect(try store.thumbnail(for: entry("a")) == Data("png".utf8))
        #expect(store.viewport(for: "a") == viewport)
    }

    @Test func aViewportFollowsItsRenamedDrawing() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.saveViewport(viewport, for: "a")

        _ = try store.rename("a", to: "b")

        #expect(store.viewport(for: "b") == viewport)
        #expect(store.viewport(for: "a") == nil)
    }

    @Test func aDuplicateStartsWithoutAViewport() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.saveViewport(viewport, for: "a")

        let copy = try store.duplicate("a")

        #expect(store.viewport(for: copy) == nil)
    }

    @Test func deletingADrawingDeletesItsViewport() throws {
        try store.save(Data("a".utf8), as: "a")
        try store.saveViewport(viewport, for: "a")

        try store.delete("a")
        try store.save(Data("a".utf8), as: "a")

        #expect(store.viewport(for: "a") == nil)
    }

    @Test func aViewportCannotBeSavedForANameThatIsNoDrawing() throws {
        #expect(throws: DrawingStore.InvalidName.self) { try store.saveViewport(viewport, for: "../a") }
        #expect(store.viewport(for: "../a") == nil)
    }

    // MARK: Changes from outside

    /// Counts the changes the store reports, from now on.
    @MainActor private func countChanges() -> (count: Counter, stop: () -> Void) {
        let count = Counter()
        let changes = store.changes()  // watching from here on
        let watching = Task { for await _ in changes { count.increment() } }
        return (count, { watching.cancel() })
    }

    @MainActor @Test func aFileAddedFromOutsideIsReported() async throws {
        let changes = countChanges()
        defer { changes.stop() }

        try Data("{}".utf8).write(to: directory.appending(path: "dropped.excalidraw"))

        #expect(await eventually { changes.count.count > 0 })
    }

    @MainActor @Test func aFileRemovedFromOutsideIsReported() async throws {
        try store.save(Data("a".utf8), as: "a")
        let changes = countChanges()
        defer { changes.stop() }

        try FileManager.default.removeItem(at: directory.appending(path: "a.excalidraw"))

        #expect(await eventually { changes.count.count > 0 })
    }

    @MainActor @Test func aFileRenamedFromOutsideIsReported() async throws {
        try store.save(Data("a".utf8), as: "a")
        let changes = countChanges()
        defer { changes.stop() }

        try FileManager.default.moveItem(
            at: directory.appending(path: "a.excalidraw"), to: directory.appending(path: "b.excalidraw"))

        #expect(await eventually { changes.count.count > 0 })
    }

    @MainActor @Test func stopsWatchingWhenNobodyListensAnyMore() async throws {
        let changes = countChanges()
        changes.stop()
        try await Task.sleep(for: .milliseconds(100))
        let before = changes.count.count

        try Data("{}".utf8).write(to: directory.appending(path: "late.excalidraw"))
        try await Task.sleep(for: .milliseconds(300))

        #expect(changes.count.count == before)
    }
}
