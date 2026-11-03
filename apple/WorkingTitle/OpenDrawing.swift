import Foundation

/// A Drawing opened from the `DrawingStore` into an `Editor`, kept saved:
/// a change is saved `autosaveDelay` after the last one, and `saveNow` saves
/// at once (leaving the Drawing, app going to the background).
@MainActor
final class OpenDrawing {
    let name: String
    let editor: Editor

    private let store: DrawingStore
    private let autosaveDelay: Duration
    /// The `Editor.revision` of what the file holds.
    private var savedRevision: UInt64
    private var pendingSave: Task<Void, Never>?

    /// Throws if the Drawing cannot be read or is not a valid Drawing; it is
    /// never overwritten then.
    init(name: String, store: DrawingStore, autosaveDelay: Duration = .seconds(1)) throws {
        let editor = Editor.withExcalifont()
        try editor.load(bytes: store.open(name))
        self.name = name
        self.editor = editor
        self.store = store
        self.autosaveDelay = autosaveDelay
        savedRevision = editor.revision()
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

    /// Saves unsaved changes now and drops the delayed save.
    func saveNow() {
        pendingSave?.cancel()
        pendingSave = nil
        guard hasUnsavedChanges else { return }
        let revision = editor.revision()
        // A failed save keeps the changes unsaved: the next change or the next
        // `saveNow` tries again.
        guard (try? store.save(editor.save(), as: name)) != nil else { return }
        savedRevision = revision
    }

    private var hasUnsavedChanges: Bool { editor.revision() != savedRevision }
}
