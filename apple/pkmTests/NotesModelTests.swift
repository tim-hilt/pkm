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

    @Test func foldersAreMadeSelectedAndNotesGoIntoTheSelectedOne() throws {
        let model = try openModel()
        let work = try model.addFolder(named: "Work", in: "")
        let plans = try model.addFolder(named: "Plans", in: work)
        #expect(plans == "Work/Plans")
        #expect(model.folderPath == plans)
        #expect(model.tree.map(\.name) == ["Work"])
        #expect(model.tree[0].children?.map(\.path) == ["Work/Plans"])
        model.add()
        let id = try #require(model.openID)
        model.save(id: id, title: "In plans", body: "")
        #expect(model.rows.map(\.id) == [id])
        model.folderPath = ""
        #expect(model.openID == nil)
        #expect(model.rows.isEmpty)
    }

    @Test func refusedFolderNamesThrowTheirReason() throws {
        let model = try openModel()
        try model.addFolder(named: "Work", in: "")
        for (name, reason) in [("", FolderNameError.Empty), ("a/b", .HasSlash), (".x", .Hidden), ("work", .Taken)] {
            do {
                try model.addFolder(named: name, in: "")
                Issue.record("\(name) was accepted")
            } catch let error as FolderNameError {
                #expect(error == reason, "\(name)")
            }
        }
        #expect(model.tree.map(\.name) == ["Work"])
    }

    @Test func renamingAFolderKeepsItAndItsNotesSelected() throws {
        let model = try openModel()
        let work = try model.addFolder(named: "Work", in: "")
        let plans = try model.addFolder(named: "Plans", in: work)
        model.add()
        let id = try #require(model.openID)
        model.save(id: id, title: "Kept", body: "")
        try model.renameFolder(work, to: "Job")
        #expect(model.folderPath == "Job/Plans")
        #expect(model.openID == id)
        #expect(model.rows.map(\.id) == [id])
        #expect(model.tree.map(\.name) == ["Job"])
        #expect(plans == "Work/Plans")
    }

    @Test func deletingAFolderSaysWhatItDeletesAndMovesTheSelectionOut() throws {
        let model = try openModel()
        let work = try model.addFolder(named: "Work", in: "")
        try model.addFolder(named: "Plans", in: work)
        model.add()
        let id = try #require(model.openID)
        model.save(id: id, title: "Gone", body: "x")
        #expect(model.summary(ofFolder: work) == "This Deletes 1 Note and 1 Folder.")
        #expect(model.summary(ofFolder: "Work/Plans") == "This Deletes 1 Note and 0 Folders.")
        model.deleteFolder(work)
        #expect(model.tree.isEmpty)
        #expect(model.folderPath == "")
        #expect(model.openID == nil)
        #expect(model.note(id: id) == nil)
    }

    @Test func aMovedNoteStaysOpenAndIsListedInItsNewFolder() throws {
        let model = try openModel()
        let work = try model.addFolder(named: "Work", in: "")
        model.add()
        let id = try #require(model.openID)
        model.save(id: id, title: "Moving", body: "x")
        model.moveNote(id: id, into: "")
        #expect(model.openID == id)
        #expect(model.rows.isEmpty)
        #expect(model.note(id: id)?.title == "Moving")
        model.folderPath = ""
        #expect(model.rows.map(\.id) == [id])
        model.moveNote(id: id, into: work)
        #expect(model.rows.isEmpty)
        model.folderPath = work
        #expect(model.rows.map(\.id) == [id])
    }

    @Test func aMovedFolderKeepsItsSelectionAndRefusalsThrowTheirReason() throws {
        let model = try openModel()
        let work = try model.addFolder(named: "Work", in: "")
        let plans = try model.addFolder(named: "Plans", in: work)
        let home = try model.addFolder(named: "Home", in: "")
        model.folderPath = plans
        try model.moveFolder(work, into: home)
        #expect(model.folderPath == "Home/Work/Plans")
        #expect(model.tree.map(\.name) == ["Home"])
        try model.addFolder(named: "Work", in: "")
        for (path, into, reason) in [
            ("Home", "Home", FolderNameError.Inside), ("Home", "Home/Work", .Inside), ("Home/Work", "", .Taken),
        ] {
            do {
                try model.moveFolder(path, into: into)
                Issue.record("\(path) into \(into) was accepted")
            } catch let error as FolderNameError {
                #expect(error == reason, "\(path) into \(into)")
            }
        }
    }

    @Test func theLastFolderAndNoteOpenAgain() throws {
        let defaults = isolatedDefaults()
        let first = try openModel(defaults: defaults)
        try first.addFolder(named: "Work", in: "")
        first.add()
        let id = try #require(first.openID)
        first.save(id: id, title: "Back", body: "")
        let again = try openModel(defaults: defaults)
        #expect(again.folderPath == "Work")
        #expect(again.openID == id)
        #expect(again.pushedPath == ["/Work", id])
        again.deleteFolder("Work")
        let gone = try openModel(defaults: defaults)
        #expect(gone.folderPath == "")
        #expect(gone.openID == nil)
        #expect(gone.pushedPath.isEmpty)
    }

    // MARK: Images

    private var png: Data { TestImages.png(width: 2, height: 2, color: { _, _ in TestImages.blue }) }

    private func fileExists(_ reference: String, in folder: String = "") -> Bool {
        FileManager.default.fileExists(atPath: root.appending(path: "Notes/\(folder)/\(reference)").path)
    }

    @Test func aPictureIsKeptBesideItsNoteAndReadBack() throws {
        let model = try openModel()
        let work = try model.addFolder(named: "Work", in: "")
        model.add()
        let id = try #require(model.openID)
        let reference = try #require(model.addImage(png, to: id))
        #expect(fileExists(reference, in: work))
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
}
