import Foundation
import Observation

/// The Notes Context's state: the Notes list and the open Note, from the core's store. It only
/// forwards to the store; every Notes rule lives in the core.
@MainActor @Observable
final class NotesModel {
    private let store: NoteStore

    /// Every Note, newest changed first.
    private(set) var rows: [ListedNote] = []

    /// The open Note's ID. Leaving a Note (opening another, or none) discards it if it is empty.
    var openID: String? {
        didSet {
            guard oldValue != openID else { return }
            if let oldValue { try? store.discardIfEmpty(id: oldValue) }
            if openID != newID { newID = nil }
            reload()
        }
    }

    /// The Note just added and not yet left: its editor takes the focus on the Title.
    private(set) var newID: String?

    init(store: NoteStore) {
        self.store = store
        reload()
    }

    /// Adds an empty Note and opens it, for its Title to be typed.
    func add() {
        guard let note = try? store.create() else { return }
        openID = note.id
        newID = note.id
        reload()
    }

    /// The Note `id` as last saved.
    func note(id: String) -> Note? { try? store.read(id: id) }

    /// Saves the Title and Body as typed.
    func save(id: String, title: String, body: String) {
        guard (try? store.edit(id: id, title: title, body: body)) != nil else { return }
        reload()
    }

    /// Whether Deleting the Note asks first: its Body has text.
    func confirmsDelete(id: String) -> Bool {
        !(note(id: id)?.body.isEmpty ?? true)
    }

    /// Deletes the Note; the open one closes.
    func delete(id: String) {
        if openID == id { openID = nil }
        try? store.delete(id: id)
        reload()
    }

    private func reload() {
        rows = (try? store.notes()) ?? []
    }
}
