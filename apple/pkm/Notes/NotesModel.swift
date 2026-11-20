import Foundation
import Observation

/// The Notes Context's state: the Folder tree with the Notes of each Folder, the selected Folder and
/// the open Note, from the core's store. It only forwards to the store; every Notes rule lives in the core.
@MainActor @Observable
final class NotesModel {
    private let store: NoteStore
    @ObservationIgnored private let defaults: UserDefaults?
    private static let folderKey = "notesLastFolder"
    private static let noteKey = "notesLastNote"

    /// The Folders at the top level, with the Folders in them.
    private(set) var tree: [Folder] = []

    /// The Notes of each Folder read since the files last changed, by path; read again on demand.
    @ObservationIgnored private var listed: [String: [ListedNote]] = [:]

    /// Grows when the files change, so that views asking `notes(in:)` read them again.
    private var changes = 0

    /// The Folders open in the tree, by path.
    var expanded: Set<String> = []

    /// What Search looks for; empty when not searching.
    var searchText = "" {
        didSet { if searchText != oldValue { reloadResults() } }
    }

    /// The Notes in any Folder that match `searchText`, Title matches first; empty when not searching.
    private(set) var results: [SearchResult] = []

    /// The selected Folder's path, where new Notes and Folders go; empty for the top level.
    /// Selecting another closes the open Note.
    var folderPath: String {
        get { selectedFolder }
        set {
            guard newValue != selectedFolder else { return }
            openID = nil
            selectedFolder = newValue
        }
    }

    /// What `folderPath` gives; set alone where the selected Folder is renamed or Moved, which
    /// leaves the open Note open.
    private var selectedFolder = "" {
        didSet {
            defaults?.set(selectedFolder, forKey: Self.folderKey)
        }
    }

    /// The open Note's ID. Leaving a Note (opening another, or none) discards it if it is empty.
    var openID: String? {
        didSet {
            guard oldValue != openID else { return }
            if let oldValue {
                try? store.releaseImages(id: oldValue, references: Array(imagesInPlay))
                try? store.discardIfEmpty(id: oldValue)
            }
            imagesInPlay = openID.flatMap { note(id: $0) }.map { Set(imageTargets(text: $0.body)) } ?? []
            if openID != newID { newID = nil }
            defaults?.set(openID, forKey: Self.noteKey)
            filesChanged()
        }
    }

    /// The Images the open Note started with or added, until it is left.
    @ObservationIgnored private var imagesInPlay: Set<String> = []

    /// The Note just added and not yet left: its editor takes the focus on the Title.
    private(set) var newID: String?

    /// How often Sync changed the files: when it grows, an open editor takes the Note as saved.
    private(set) var syncs = 0

    /// `defaults` holds the last Folder and Note, which open again unless they are gone; `nil`
    /// remembers nothing.
    init(store: NoteStore, defaults: UserDefaults? = nil) {
        self.store = store
        self.defaults = defaults
        reloadTree()
        if let path = defaults?.string(forKey: Self.folderKey), (try? store.notes(folder: path)) != nil {
            selectedFolder = path
        }
        if let id = defaults?.string(forKey: Self.noteKey), notes(in: folderPath).contains(where: { $0.id == id }) {
            openID = id
        }
        expand(folderPath)
    }

    // MARK: Navigation

    /// What is pushed over the top level, for a narrow window: each Folder down to the selected one
    /// as `"/" + path`, then the open Note's ID. (Neither a Note ID nor a Folder name starts with a `/`.)
    var pushedPath: [String] {
        get { Self.ancestry(of: folderPath).map(Self.route(to:)) + (openID.map { [$0] } ?? []) }
        set {
            let folder = newValue.last { $0.hasPrefix("/") }.map { String($0.dropFirst()) } ?? ""
            let note = newValue.last.flatMap { $0.hasPrefix("/") ? nil : $0 }
            folderPath = folder
            openID = note
        }
    }

    /// The selected row of the tree, for a wide window: the open Note's ID, or the selected Folder
    /// as its route. Selecting a Note selects its Folder and opens the Folders around it; selecting
    /// none (a click beside the rows) selects the top level, unless Search shows other rows.
    var selection: String? {
        get { openID ?? (folderPath.isEmpty ? nil : Self.route(to: folderPath)) }
        set {
            guard let newValue else {
                guard searchText.isEmpty else { return }
                folderPath = ""
                openID = nil
                return
            }
            if newValue.hasPrefix("/") {
                folderPath = String(newValue.dropFirst())
                return
            }
            let folder = folder(ofNote: newValue) ?? folderPath
            selectedFolder = folder
            expand(folder)
            openID = newValue
        }
    }

    /// Shows the Folder at `path`.
    static func route(to path: String) -> String { "/" + path }

    /// The paths from the top level down to the Folder at `path`, without the top level.
    private static func ancestry(of path: String) -> [String] {
        guard !path.isEmpty else { return [] }
        var paths: [String] = []
        for name in path.split(separator: "/") {
            paths.append(paths.last.map { "\($0)/\(name)" } ?? String(name))
        }
        return paths
    }

    /// Opens the Folder at `path` in the tree, and every Folder around it.
    private func expand(_ path: String) {
        expanded.formUnion(Self.ancestry(of: path))
    }

    // MARK: Notes

    /// Adds an empty Note to the selected Folder and opens it, for its Title to be typed.
    func add() {
        guard let note = try? store.create(folder: folderPath) else { return }
        newID = note.id
        expand(folderPath)
        openID = note.id
    }

    /// The Note `id` as last saved.
    func note(id: String) -> Note? { try? store.read(id: id) }

    /// Saves the Title and Body as typed.
    func save(id: String, title: String, body: String) {
        guard (try? store.edit(id: id, title: title, body: body)) != nil else { return }
        if let folder = listed.first(where: { $0.value.contains { $0.id == id } })?.key {
            listed[folder] = nil
            changes += 1
        }
        reloadResults()
    }

    /// Whether Deleting the Note asks first: its Body has text.
    func confirmsDelete(id: String) -> Bool {
        !(note(id: id)?.body.isEmpty ?? true)
    }

    /// Deletes the Note; the open one closes.
    func delete(id: String) {
        if openID == id { openID = nil }
        try? store.delete(id: id)
        filesChanged()
    }

    // MARK: Images

    /// Keeps a picture beside the Note `id`, as PNG unless it is a PNG, JPEG, GIF or WebP; the
    /// reference for its Body. Nil if it is no image. Leaving the Note Deletes it if the Body
    /// doesn't show it.
    func addImage(_ picture: Data, to id: String) -> String? {
        let reference = StoredImage.bytes(of: picture).flatMap { try? store.addImage(id: id, bytes: $0) }
        if let reference, id == openID { imagesInPlay.insert(reference) }
        return reference
    }

    /// The bytes of the Image `reference` names beside the Note `id`.
    func image(_ reference: String, of id: String) -> Data? { try? store.readImage(id: id, reference: reference) }

    /// What the Body editor of the Note `id` keeps its Images with.
    func images(for id: String) -> ImageSource {
        ImageSource(add: { [self] in addImage($0, to: id) }, read: { [self] in image($0, of: id) })
    }

    // MARK: Folders

    /// Makes a Folder in the Folder at `parent` and selects it; throws a `FolderNameError` for a
    /// name the core refuses.
    @discardableResult
    func addFolder(named name: String, in parent: String) throws -> String {
        let path = try store.createFolder(parent: parent, name: name)
        reloadTree()
        expand(parent)
        folderPath = path
        return path
    }

    /// Renames the Folder at `path` (throwing as `addFolder` does); a selected Folder, or one around it, stays selected.
    func renameFolder(_ path: String, to name: String) throws {
        relocateFolder(path, to: try store.renameFolder(path: path, name: name))
    }

    /// Moves the Folder at `path` into the Folder at `parent` (empty for the top level), throwing a
    /// `FolderNameError` for a Move the core refuses; a selected Folder, or one around it, stays selected.
    func moveFolder(_ path: String, into parent: String) throws {
        relocateFolder(path, to: try store.moveFolder(path: path, into: parent))
    }

    /// Moves the Note into the Folder at `folder` (empty for the top level). An open Note stays
    /// open, even where the selected Folder doesn't list it.
    func moveNote(id: String, into folder: String) {
        guard (try? store.moveNote(id: id, folder: folder)) != nil else { return }
        filesChanged()
    }

    /// The selection follows a Folder that is now at `new`, and the tree is read again.
    private func relocateFolder(_ old: String, to new: String) {
        if selectedFolder == old || selectedFolder.hasPrefix(old + "/") {
            selectedFolder = new + String(selectedFolder.dropFirst(old.count))
        }
        expanded = Set(expanded.map { $0 == old || $0.hasPrefix(old + "/") ? new + $0.dropFirst(old.count) : $0 })
        reloadTree()
        filesChanged()
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

    /// The Folders in the Folder at `path` (empty for the top level), by name.
    func folders(in path: String) -> [Folder] {
        path.isEmpty ? tree : folder(at: path)?.folders ?? []
    }

    /// The Notes in the Folder at `path` (empty for the top level), newest changed first.
    func notes(in path: String) -> [ListedNote] {
        _ = changes
        if let notes = listed[path] { return notes }
        let notes = (try? store.notes(folder: path)) ?? []
        listed[path] = notes
        return notes
    }

    /// The path of the Folder that holds the Note `id`, looked for where it was last seen first.
    private func folder(ofNote id: String) -> String? {
        if let found = results.first(where: { $0.id == id }) { return found.folder }
        if let seen = listed.first(where: { $0.value.contains { $0.id == id } }) { return seen.key }
        func paths(_ level: [Folder]) -> [String] { level.flatMap { [$0.path] + paths($0.folders) } }
        return ([""] + paths(tree)).first { path in notes(in: path).contains { $0.id == id } }
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

    /// Deletes the Folder with everything in it; the selection moves out of it, and an open Note in
    /// it closes.
    func deleteFolder(_ path: String) {
        try? store.deleteFolder(path: path)
        if let openID, note(id: openID) == nil { self.openID = nil }
        if selectedFolder == path || selectedFolder.hasPrefix(path + "/") {
            selectedFolder = path.contains("/") ? String(path[..<path.lastIndex(of: "/")!]) : ""
        }
        reloadTree()
        filesChanged()
    }

    /// Sync wrote or deleted Note files: the tree and lists show what is there now. A Folder it
    /// took leaves the selection for the nearest one still there, and an open Note it Deleted
    /// closes; one it Moved stays open.
    func syncDidChange() {
        reloadTree()
        var path = selectedFolder
        while !path.isEmpty, folder(at: path) == nil {
            path = path.lastIndex(of: "/").map { String(path[..<$0]) } ?? ""
        }
        if path != selectedFolder { selectedFolder = path }
        if let openID, note(id: openID) == nil { self.openID = nil }
        syncs += 1
        filesChanged()
    }

    private func reloadTree() {
        tree = (try? store.folders()) ?? []
    }

    /// Notes were added, Deleted or Moved: lists read them again.
    private func filesChanged() {
        listed = [:]
        changes += 1
        reloadResults()
    }

    private func reloadResults() {
        results = searchText.isEmpty ? [] : (try? store.search(query: searchText)) ?? []
    }
}

extension Folder {
    /// For an outline: nil where there are no Folders to show.
    var children: [Folder]? { folders.isEmpty ? nil : folders }
}
