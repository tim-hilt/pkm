import Foundation

extension NotesModel {
    /// The Notes folder in the app's Documents, which Files and Finder show.
    static let documentsFolder = URL.documentsDirectory.appending(path: "Notes", directoryHint: .isDirectory)

    /// The Notes for this launch: the Notes folder. For UI tests and screenshots (`-UITesting…`) it
    /// is empty and temporary, with sample Notes when launched with `-UITestingOpenNotes`, and
    /// also the "Reading list" open with `-UITestingOpenNote`.
    @MainActor
    static func forLaunch() -> NotesModel {
        let arguments = ProcessInfo.processInfo.arguments
        guard arguments.contains(where: { $0.hasPrefix("-UITesting") }) else {
            return NotesModel(store: openOrTemporary(documentsFolder), defaults: .standard)
        }
        let store = openOrTemporary(temporaryFolder())
        let opensNote = arguments.contains("-UITestingOpenNote")
        if opensNote || arguments.contains("-UITestingOpenNotes") {
            let samples = [
                ("Reply to Anna", "About the trip: *train* or **plane**?\n"),
                (
                    "Reading list",
                    """
                    ## Next up
                    - `Rust` in **Action**
                    - A [book](https://example.com) on *design*
                    1. First
                    1. Second
                    > Read a little every day
                    """
                ),
                ("", "Buy oat milk\nand bread"),
            ]
            for (title, body) in samples.reversed() {
                guard let note = try? store.create() else { continue }
                _ = try? store.edit(id: note.id, title: title, body: body)
            }
        }
        let model = NotesModel(store: store)
        if opensNote { model.openID = model.notes.first { $0.title == "Reading list" }?.id }
        return model
    }

    private static func temporaryFolder() -> URL {
        FileManager.default.temporaryDirectory.appending(
            path: "pkmUITests-\(UUID().uuidString)/Notes", directoryHint: .isDirectory)
    }

    /// The store in `folder`, created if needed. Where that fails, a temporary one: the app still
    /// runs, its Notes just don't stay.
    private static func openOrTemporary(_ folder: URL) -> NoteStore {
        (try? NoteStore.open(root: folder.path)) ?? (try! NoteStore.open(root: temporaryFolder().path))
    }
}
