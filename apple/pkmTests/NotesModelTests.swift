import Foundation
import Testing

@testable import pkm

@MainActor @Suite struct NotesModelTests {
    let root = FileManager.default.temporaryDirectory
        .appending(path: UUID().uuidString, directoryHint: .isDirectory)

    func openModel() throws -> NotesModel {
        NotesModel(store: try NoteStore.open(root: root.appending(path: "Notes").path))
    }

    @Test func aNewNoteIsOpenWithItsTitleToFocusAndLeavingItEmptyDiscardsIt() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        #expect(model.newID == id)
        #expect(model.rows.map(\.id) == [id])
        model.openID = nil
        #expect(model.rows.isEmpty)
        #expect(model.newID == nil)
    }

    @Test func editsAreSavedAsTypedAndListed() throws {
        let model = try openModel()
        model.add()
        let id = try #require(model.openID)
        model.save(id: id, title: "A *title*", body: "- first line\n")
        model.openID = nil
        #expect(model.rows.map(\.title) == ["A *title*"])
        #expect(model.rows.map(\.preview) == ["first line"])
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
        #expect(model.rows.map(\.id) == [stub])
    }
}
