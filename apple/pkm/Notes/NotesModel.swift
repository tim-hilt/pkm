import Foundation
import Observation

/// The Notes Context's state: the Folder tree, the selected Folder's Notes and the open Note, from
/// the core's store. It only forwards to the store; every Notes rule lives in the core.
@MainActor @Observable
final class NotesModel {
    private let store: NoteStore
    @ObservationIgnored private let defaults: UserDefaults?
    private static let folderKey = "notesLastFolder"
    private static let noteKey = "notesLastNote"

    /// The Folders at the top level, with the Folders in them.
    private(set) var tree: [Folder] = []

    /// The Notes of the selected Folder, newest changed first.
    private(set) var rows: [ListedNote] = []

    /// The selected Folder's path; empty for the top level. Selecting another closes the open Note.
    var folderPath = "" {
        didSet {
            guard oldValue != folderPath else { return }
            openID = nil
            defaults?.set(folderPath, forKey: Self.folderKey)
            reloadRows()
        }
    }

    /// The open Note's ID. Leaving a Note (opening another, or none) discards it if it is empty.
    var openID: String? {
        didSet {
            guard oldValue != openID else { return }
            if let oldValue { try? store.discardIfEmpty(id: oldValue) }
            if openID != newID { newID = nil }
            defaults?.set(openID, forKey: Self.noteKey)
            reloadRows()
        }
    }

    /// The Note just added and not yet left: its editor takes the focus on the Title.
    private(set) var newID: String?

    /// Whether the selected Folder's Notes are pushed over the tree, where the columns don't fit.
    private(set) var showsFolder = false

    /// `defaults` holds the last Folder and Note, which open again unless they are gone; `nil`
    /// remembers nothing.
    init(store: NoteStore, defaults: UserDefaults? = nil) {
        self.store = store
        self.defaults = defaults
        reloadTree()
        if let path = defaults?.string(forKey: Self.folderKey), (try? store.notes(folder: path)) != nil {
            folderPath = path
        }
        reloadRows()
        if let id = defaults?.string(forKey: Self.noteKey), rows.contains(where: { $0.id == id }) {
            openID = id
        }
        showsFolder = !folderPath.isEmpty || openID != nil
    }

    // MARK: Pushed screens

    /// What is pushed over the tree, for a narrow window: the selected Folder as `"/" + path`, then
    /// the open Note's ID. (Neither a Note ID nor a Folder name starts with a `/`.)
    var pushedPath: [String] {
        get { (showsFolder ? ["/" + folderPath] : []) + (openID.map { [$0] } ?? []) }
        set {
            showsFolder = !newValue.isEmpty
            if let folder = newValue.first, folder.hasPrefix("/") { folderPath = String(folder.dropFirst()) }
            openID = newValue.count > 1 ? newValue.last : nil
        }
    }

    /// Pushes the Notes of the Folder at `path`.
    static func route(to path: String) -> String { "/" + path }

    // MARK: Notes

    /// Adds an empty Note to the selected Folder and opens it, for its Title to be typed.
    func add() {
        guard let note = try? store.create(folder: folderPath) else { return }
        newID = note.id
        showsFolder = true
        openID = note.id
    }

    /// The Note `id` as last saved.
    func note(id: String) -> Note? { try? store.read(id: id) }

    /// Saves the Title and Body as typed.
    func save(id: String, title: String, body: String) {
        guard (try? store.edit(id: id, title: title, body: body)) != nil else { return }
        reloadRows()
    }

    /// Whether Deleting the Note asks first: its Body has text.
    func confirmsDelete(id: String) -> Bool {
        !(note(id: id)?.body.isEmpty ?? true)
    }

    /// Deletes the Note; the open one closes.
    func delete(id: String) {
        if openID == id { openID = nil }
        try? store.delete(id: id)
        reloadRows()
    }

    // MARK: Folders

    /// Makes a Folder in the Folder at `parent` and selects it; throws a `FolderNameError` for a
    /// name the core refuses.
    @discardableResult
    func addFolder(named name: String, in parent: String) throws -> String {
        let path = try store.createFolder(parent: parent, name: name)
        reloadTree()
        folderPath = path
        return path
    }

    /// Renames the Folder at `path` (throwing as `addFolder` does); a selected Folder, or one around it, stays selected.
    func renameFolder(_ path: String, to name: String) throws {
        let renamed = try store.renameFolder(path: path, name: name)
        let open = openID
        if folderPath == path || folderPath.hasPrefix(path + "/") {
            folderPath = renamed + String(folderPath.dropFirst(path.count))
            openID = open
        }
        reloadTree()
    }

    /// The Folder at `path` in the tree.
    func folder(at path: String) -> Folder? {
        func find(in level: [Folder]) -> Folder? {
            for folder in level {
                if folder.path == path { return folder }
                if path.hasPrefix(folder.path + "/"), let inner = find(in: folder.folders) { return inner }
            }
            return nil
        }
        return find(in: tree)
    }

    /// What Deleting the Folder at `path` would Delete, for the confirmation.
    func summary(ofFolder path: String) -> String {
        let counts = (try? store.count(path: path)) ?? Counts(notes: 0, folders: 0)
        func part(_ n: UInt32, _ one: String, _ many: String) -> String { n == 1 ? "1 \(one)" : "\(n) \(many)" }
        if counts.notes == 0 && counts.folders == 0 { return String(localized: "It is empty.") }
        let notes = part(counts.notes, "Note", "Notes")
        let folders = part(counts.folders, "Folder", "Folders")
        return String(localized: "This Deletes \(notes) and \(folders).")
    }

    /// Deletes the Folder with everything in it; the selection moves out of it.
    func deleteFolder(_ path: String) {
        try? store.deleteFolder(path: path)
        if folderPath == path || folderPath.hasPrefix(path + "/") {
            folderPath = path.contains("/") ? String(path[..<path.lastIndex(of: "/")!]) : ""
        }
        reloadTree()
        reloadRows()
    }

    private func reloadTree() {
        tree = (try? store.folders()) ?? []
    }

    private func reloadRows() {
        rows = (try? store.notes(folder: folderPath)) ?? []
    }
}

extension Folder {
    /// For an outline: nil where there are no Folders to show.
    var children: [Folder]? { folders.isEmpty ? nil : folders }
}
