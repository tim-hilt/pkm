import Foundation
import Testing

@testable import WorkingTitle

/// Seam 3: the `DrawingStore` against a temporary directory. Drawings are
/// `<name>.excalidraw` files in it; the tests look at the directory itself to
/// see what a user would see in Files or Finder.
@Suite struct DrawingStoreTests {
    private let directory: URL
    private let store: DrawingStore

    init() throws {
        directory = FileManager.default.temporaryDirectory
            .appending(path: "DrawingStoreTests-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        store = DrawingStore(directory: directory)
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

    // MARK: Open and save

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
}
