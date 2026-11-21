import Foundation
import Testing

@testable import pkm

@MainActor @Suite struct NotesModelTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: UUID().uuidString, directoryHint: .isDirectory)

    func openModel(defaults: UserDefaults? = nil) throws -> NotesModel {
        NotesModel(store: try NoteStore.open(root: root.appending(path: "Notes").path), defaults: defaults)
    }

    func isolatedDefaults() -> UserDefaults {
        UserDefaults(suiteName: "NotesModelTests-\(UUID().uuidString)")!
    }

    @Test func aNewNoteIsOpenWithItsTitleToFocusAndLeavingItEmptyDiscardsIt() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        #expect(model.newID == id)
        #expect(model.notes.map(\.id) == [id])
        model.openID = nil
        #expect(model.notes.isEmpty)
        #expect(model.newID == nil)
    }

    @Test func editsAreSavedAsTypedAndListed() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        model.save(id: id, title: "A *title*", body: "- first line\n")
        model.openID = nil
        #expect(model.notes.map(\.title) == ["A *title*"])
        #expect(model.notes.map(\.preview) == ["first line"])
        #expect(model.note(id: id)?.body == "- first line\n")
    }

    @Test func aNoteIsDeletedAndOnlyAskedAboutWithABody() throws {
        let model = try openModel()
        model.add()
        let stub = try #require(model.openID)
        model.save(id: stub, title: "Stub", body: "")
        #expect(!model.confirmsDelete(id: stub))
        model.add()
        let written = try #require(model.openID)
        model.save(id: written, title: "", body: "text")
        #expect(model.confirmsDelete(id: written))
        model.delete(id: written)
        #expect(model.openID == nil)
        #expect(model.notes.map(\.id) == [stub])
    }

    @Test func syncDeletingTheOpenNoteClosesItAndEveryNoteIsListedFlat() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        model.save(id: id, title: "Gone", body: "text")
        let syncs = model.syncs
        // What a Sync step does for a Note Deleted elsewhere.
        try FileManager.default.removeItem(at: root.appending(path: "Notes/\(id).md"))
        model.syncDidChange()
        #expect(model.openID == nil)
        #expect(model.notes.isEmpty)
        #expect(model.syncs == syncs + 1)
    }

    @Test func aNoteInASubdirectoryIsNotListed() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        let notes = root.appending(path: "Notes")
        let work = notes.appending(path: "Work")
        try FileManager.default.createDirectory(at: work, withIntermediateDirectories: true)
        try FileManager.default.copyItem(at: notes.appending(path: "\(id).md"), to: work.appending(path: "\(id).md"))
        model.save(id: id, title: "Kept", body: "x")
        #expect(model.notes.map(\.id) == [id])
        #expect(FileManager.default.fileExists(atPath: work.appending(path: "\(id).md").path))
    }

    @Test func pushingANoteOpensItAndPoppingClosesIt() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        model.save(id: id, title: "Pushed", body: "x")
        model.pushedPath = []
        #expect(model.openID == nil)
        model.pushedPath = [id]
        #expect(model.openID == id)
        #expect(model.pushedPath == [id])
    }

    @Test func theLastNoteOpensAgainUnlessItIsGone() throws {
        let defaults = isolatedDefaults()
        let first = try openModel(defaults: defaults)
        first.add()
        let id = try #require(first.openID)
        first.save(id: id, title: "Back", body: "")
        let again = try openModel(defaults: defaults)
        #expect(again.openID == id)
        #expect(again.pushedPath == [id])
        again.delete(id: id)
        let gone = try openModel(defaults: defaults)
        #expect(gone.openID == nil)
        #expect(gone.pushedPath.isEmpty)
    }

    // MARK: Images

    private var png: Data { TestImages.png(width: 2, height: 2, color: { _, _ in TestImages.blue }) }

    private func fileExists(_ reference: String) -> Bool {
        FileManager.default.fileExists(atPath: root.appending(path: "Notes/\(reference)").path)
    }

    @Test func aPictureIsKeptBesideItsNoteAndReadBack() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        let reference = try #require(model.addImage(png, to: id))
        #expect(fileExists(reference))
        #expect(model.image(reference, of: id) == png)
        #expect(model.addImage(Data("text".utf8), to: id) == nil)
        #expect(model.image("missing.png", of: id) == nil)
    }

    @Test func anImageRemovedFromTheBodyIsDeletedOnLeavingTheNote() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        let kept = try #require(model.addImage(png, to: id))
        let removed = try #require(model.addImage(png, to: id))
        model.save(id: id, title: "", body: "![](\(kept))\n\n![](\(removed))")
        model.save(id: id, title: "", body: "![](\(kept))")
        #expect(fileExists(removed), "released on leaving, not before")
        model.openID = nil
        #expect(fileExists(kept))
        #expect(!fileExists(removed))
    }

    @Test func anImageRemovedInEarlierVisitIsReleasedOnTheNextLeave() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        let image = try #require(model.addImage(png, to: id))
        model.save(id: id, title: "", body: "![](\(image))")
        model.openID = nil
        model.openID = id
        model.save(id: id, title: "", body: "text")
        model.openID = nil
        #expect(!fileExists(image))
    }

    @Test func deletingANoteDeletesItsImages() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        let image = try #require(model.addImage(png, to: id))
        model.save(id: id, title: "", body: "![](\(image))")
        model.delete(id: id)
        #expect(!fileExists(image))
    }

    @Test func searchFindsNotesAsTheTextChanges() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        model.save(id: id, title: "Plan", body: "Visit the Café\n")
        #expect(model.results.isEmpty)
        model.searchText = "cafe"
        #expect(model.results.map(\.id) == [id])
        #expect(model.results.first?.snippet == "Visit the Café")
        #expect(model.results.first.map { $0.matchedStart..<$0.matchedEnd } == 10..<14)
        model.searchText = "cafes"
        #expect(model.results.isEmpty)
        model.searchText = ""
        #expect(model.results.isEmpty)
    }

    @Test func aTitleMatchComesBeforeABodyMatch() throws {
        let model = try openModel()
        model.add()
        let body = try #require(model.openID)
        model.save(id: body, title: "x", body: "tea")
        model.add()
        let title = try #require(model.openID)
        model.save(id: title, title: "Tea", body: "")
        model.searchText = "tea"
        #expect(model.results.map(\.id) == [title, body])
        #expect(model.results.map(\.titleMatched) == [true, false])
    }
}
