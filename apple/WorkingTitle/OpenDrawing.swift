import Foundation

/// A Drawing opened from the `DrawingStore` into an `Editor`, kept saved:
/// a change is saved `autosaveDelay` after the last one, and `saveNow` saves
/// at once (leaving the Drawing, app going to the background).
///
/// It opens at the viewport it was left at, which `saveNow` keeps in the
/// store's cache (not in the file, so panning and zooming never change the
/// Drawing); one that was never left opens at the default viewport.
@MainActor
final class OpenDrawing {
    let name: String
    let editor: Editor

    private let store: DrawingStore
    private let autosaveDelay: Duration
    /// The `Editor.revision` of what the file holds.
    private var savedRevision: UInt64
    private var pendingSave: Task<Void, Never>?
    /// The viewport the cache holds, or the default if it holds none.
    private var savedViewport: ViewportState

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
    }

    private var hasUnsavedChanges: Bool { editor.revision() != savedRevision }
}
