import Foundation
import Observation

/// A Drawing opened from the `DrawingStore` into an `Editor`, kept saved:
/// a change is saved `autosaveDelay` after the last one, and `saveNow` saves
/// at once (leaving the Drawing, app going to the background).
///
/// It opens at the viewport it was left at, which `saveNow` keeps in the
/// store's cache (not in the file, so panning and zooming never change the
/// Drawing); one that was never left opens at the default viewport.
///
/// The folder is the single source of truth, so the file may change under an
/// open Drawing; `syncWithFile` (run before every save, and by whoever notices
/// the app coming back or the folder changing) follows it: a file changed
/// outside (by Sync, Files, Finder) is merged into the Scene live, without
/// undo steps and keeping Selection, Text view and viewport, so that unsaved
/// edits and the changes outside both survive; a file renamed outside is
/// followed, and a file deleted outside is never written again, unless
/// Cloud Backing is on (`keepsDeletedDrawingOpen`): then the Drawing stays
/// open and the next edit brings it back, as an edit wins over a delete.
@MainActor @Observable
final class OpenDrawing {
    /// Follows a rename outside the app.
    private(set) var name: String
    @ObservationIgnored let editor: Editor
    /// Called once when the file was deleted outside the app: the Drawing is
    /// gone, unsaved edits are dropped and nothing is saved any more.
    @ObservationIgnored var onDeleted: (() -> Void)?
    /// Called after every save of the Drawing's file.
    @ObservationIgnored var onSaved: (() -> Void)?
    /// Whether a file deleted outside leaves the Drawing open (Cloud Backing
    /// deletes files when another Device did) instead of closing it.
    @ObservationIgnored var keepsDeletedDrawingOpen = false

    @ObservationIgnored private let store: DrawingStore
    @ObservationIgnored private let autosaveDelay: Duration
    /// The `Editor.revision` of what the file holds.
    @ObservationIgnored private var savedRevision: UInt64
    @ObservationIgnored private var pendingSave: Task<Void, Never>?
    /// The viewport the cache holds, or the default if it holds none.
    @ObservationIgnored private var savedViewport: ViewportState
    /// The file as last read or written.
    @ObservationIgnored private var savedStamp: DrawingStore.FileStamp?
    @ObservationIgnored private var isDeleted = false

    /// What `syncWithFile` found.
    enum Outcome: Equatable {
        case unchanged
        /// The file's changes were merged into the Scene.
        case merged
        case renamed(to: String)
        case deleted
        /// The file is gone but the Drawing stays open (`keepsDeletedDrawingOpen`).
        case removed
    }

    /// Throws if the Drawing cannot be read or is not a valid Drawing; it is
    /// never overwritten then.
    init(name: String, store: DrawingStore, autosaveDelay: Duration = .seconds(1)) throws {
        let editor = Editor.withExcalifont()
        let stamp = store.stamp(of: name)
        try editor.load(bytes: store.open(name))
        savedStamp = stamp
        self.name = name
        self.editor = editor
        self.store = store
        self.autosaveDelay = autosaveDelay
        savedRevision = editor.revision()
        if let viewport = store.viewport(for: name) { editor.setViewport(viewport: viewport) }
        savedViewport = editor.viewport()
    }

    /// Call after input or a command reached the editor. Cheap when the
    /// Scene did not change (panning, zooming).
    func editorDidChange() {
        guard hasUnsavedChanges else { return }
        pendingSave?.cancel()
        pendingSave = Task { [weak self, autosaveDelay] in
            try? await Task.sleep(for: autosaveDelay)
            guard !Task.isCancelled else { return }
            self?.saveNow()
        }
    }

    /// Brings the Drawing in line with its file if that was changed, renamed
    /// or deleted outside the app (see the type's comment). Cheap when it was
    /// not: one stat.
    @discardableResult
    func syncWithFile() -> Outcome {
        guard !isDeleted else { return .deleted }
        guard let stamp = store.stamp(of: name) else { return followRename() }
        guard stamp != savedStamp else { return .unchanged }
        guard let bytes = try? store.open(name) else { return .unchanged }
        let wasSaved = !hasUnsavedChanges
        // A file that is not a Drawing is left alone; our next save replaces it.
        guard (try? editor.mergeSynced(bytes: bytes)) != nil else { return .unchanged }
        // With nothing of ours unsaved the Scene now is what the file holds;
        // otherwise the next save writes the merged Scene.
        if wasSaved { savedRevision = editor.revision() }
        savedStamp = stamp
        return .merged
    }

    /// Sync wrote or deleted the files of `names`: if it renamed this Drawing
    /// (its file is gone and exactly one of the files Sync wrote is the same
    /// Drawing) it is followed to the new name and merged with it.
    func syncDidChange(_ names: [String]) {
        guard store.stamp(of: name) == nil, !isDeleted else { return }
        let ours = editor.save()
        let same = names.filter { candidate in
            candidate != name && store.stamp(of: candidate) != nil
                && ((try? store.open(candidate)).map { areSameDrawing(a: ours, b: $0) } ?? false)
        }
        guard same.count == 1, let newName = same.first else { return }
        store.moveCaches(of: name, to: newName)
        name = newName
        savedStamp = nil  // so that the next sync merges the new file
        syncWithFile()
    }

    /// The file is gone from `name`: renamed if it is in the folder under
    /// another name, otherwise deleted.
    private func followRename() -> Outcome {
        if let stamp = savedStamp, let newName = store.name(matching: stamp), newName != name {
            store.moveCaches(of: name, to: newName)
            name = newName
            return .renamed(to: newName)
        }
        savedStamp = nil
        if keepsDeletedDrawingOpen { return .removed }
        isDeleted = true
        pendingSave?.cancel()
        pendingSave = nil
        onDeleted?()
        return .deleted
    }

    /// Runs `syncWithFile` whenever the folder changes, until the calling
    /// task is cancelled. Meant for as long as the Drawing is shown.
    func followChangesFromOutside() async {
        let changes = store.changes()
        syncWithFile()  // after the watching starts, so that no change falls in between
        for await _ in changes { syncWithFile() }
    }

    /// Leaves the Drawing: a text being edited is committed as typed (its text
    /// view would end the editing only once the Drawing is gone), and
    /// everything is saved.
    func close() {
        editor.commitText()
        saveNow()
    }

    /// Saves unsaved changes and the viewport now and drops the delayed save.
    func saveNow() {
        pendingSave?.cancel()
        pendingSave = nil
        // Never write to a file that is not the Drawing's any more.
        guard syncWithFile() != .deleted else { return }
        saveViewport()
        saveScene()
    }

    /// A failure to cache costs only the viewport: the Drawing opens at the
    /// default or at an older one.
    private func saveViewport() {
        let viewport = editor.viewport()
        guard viewport != savedViewport, (try? store.saveViewport(viewport, for: name)) != nil else { return }
        savedViewport = viewport
    }

    private func saveScene() {
        guard hasUnsavedChanges else { return }
        let revision = editor.revision()
        // A failed save keeps the changes unsaved: the next change or the next
        // `saveNow` tries again.
        guard (try? store.save(editor.save(), as: name)) != nil else { return }
        savedRevision = revision
        savedStamp = store.stamp(of: name)
        onSaved?()
    }

    private var hasUnsavedChanges: Bool { editor.revision() != savedRevision }
}
