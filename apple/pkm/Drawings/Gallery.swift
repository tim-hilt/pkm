import CoreTransferable
import Foundation
import Observation
import UniformTypeIdentifiers

/// The Gallery: the Drawings of a `DrawingStore` and which of them is open.
/// Opening one shows it in the Canvas; going back saves it and lists it again.
@MainActor @Observable
final class Gallery {
    /// The Drawings, last modified first.
    private(set) var drawings: [DrawingStore.Entry] = []
    /// The Drawing shown in the Canvas, or `nil` while the Gallery is shown.
    private(set) var open: OpenDrawing?
    /// What went wrong with the last attempt to change a Drawing, to
    /// tell the user.
    private(set) var failure: String?

    /// What Cloud Backing is doing, as it tells (`ContentView` passes it on).
    var syncProgress: SyncProgress?
    var syncStatus: SyncStatus?

    /// "Syncing 3 of 40 Drawings" while Drawings transfer.
    var syncText: String? {
        syncProgress.map { String(localized: "Syncing \($0.done) of \($0.total) Drawings") }
    }

    /// Whether the Gallery shows its warning icon: Sync is blocked because
    /// access was lost or the cloud is full.
    var showsSyncWarning: Bool { syncStatus == .accessLost || syncStatus == .full }

    /// The pictures of the Drawings for their cards.
    @ObservationIgnored let thumbnails: Thumbnails

    @ObservationIgnored private let store: DrawingStore

    /// Called when a Drawing changed on this Device (made, saved, renamed,
    /// deleted here or outside the app), so that Cloud Backing can Sync.
    @ObservationIgnored var onLocalChange: (() -> Void)?

    /// Whether Cloud Backing is on: a Drawing deleted by Sync while open then
    /// stays open (an edit wins over a delete).
    @ObservationIgnored var isCloudBacked = false {
        didSet { open?.keepsDeletedDrawingOpen = isCloudBacked }
    }

    /// Also clears out the caches of Drawings deleted in Files or Finder
    /// since the last launch.
    init(store: DrawingStore) {
        self.store = store
        thumbnails = Thumbnails(store: store)
        store.removeOrphanedCaches()
        reload()
    }

    /// The folder of the Drawings.
    var drawingsDirectory: URL { store.directory }

    /// Where Sync remembers what it Synced: in Application Support, not in the
    /// Drawings folder. Nil for the temporary stores of tests, which never Sync.
    var syncStateDirectory: URL? {
        store.directory == DrawingStore.documentsFolder
            ? .applicationSupportDirectory.appending(path: "Sync", directoryHint: .isDirectory) : nil
    }

    func reload() {
        let listed = (try? store.list()) ?? []
        if listed != drawings {
            drawings = listed
            onLocalChange?()
        }
    }

    /// Sync wrote or deleted the files of these Drawings: lists again (the
    /// thumbnails of changed ones follow their new modification dates) and
    /// clears the caches of Drawings that are gone.
    func syncDidChange(_ names: [String]) {
        open?.syncDidChange(names)
        store.removeOrphanedCaches()
        reload()
    }

    /// Lists the Drawings now and again whenever they are added, removed or
    /// renamed outside the app (Files, Finder), until the calling task is
    /// cancelled. Meant for as long as the Gallery is shown: the folder is the
    /// single source of truth.
    func followChangesFromOutside() async {
        let changes = store.changes()
        reload()  // after the watching starts, so that no change falls in between
        for await _ in changes { reload() }
    }

    /// Makes a new empty Drawing and opens it.
    func createDrawing() {
        attempt("Could not create a Drawing.") {
            let name = try store.create(contents: Editor().save())
            onLocalChange?()
            show(try OpenDrawing(name: name, store: store))
        }
    }

    /// The Drawing's `.excalidraw` file to share, exactly as stored, so it
    /// opens unchanged on excalidraw.com. The open Drawing is saved first.
    /// Tells the user and returns `nil` if there is no such file.
    func exportFile(named name: String) -> URL? {
        if open?.name == name { open?.saveNow() }
        var url: URL?
        attempt("Could not export “\(name)”.") { url = try store.fileURL(for: name) }
        return url
    }

    /// Gives a Drawing another name; tells the user if that name is taken or
    /// cannot be used.
    func renameDrawing(named name: String, to newName: String) {
        attempt("Could not rename “\(name)”.") {
            _ = try store.rename(name, to: newName)
            reload()
        }
    }

    func duplicateDrawing(named name: String) {
        attempt("Could not duplicate “\(name)”.") {
            _ = try store.duplicate(name)
            reload()
        }
    }

    /// Deletes a Drawing for good; the caller confirms with the user first.
    func deleteDrawing(named name: String) {
        attempt("Could not delete “\(name)”.") {
            try store.delete(name)
            reload()
        }
    }

    /// Opens the Drawing with the Item ID `id` (an `Entry.id`).
    func openDrawing(itemID id: String) {
        attempt("Could not open the Drawing.") {
            guard let name = try store.name(forItemID: id) else { throw CocoaError(.fileNoSuchFile) }
            show(try OpenDrawing(name: name, store: store))
        }
    }

    /// Opens the Drawing a link names; an unknown Item ID leaves the Gallery shown.
    func openDrawing(linkedItemID id: String) {
        guard (try? store.name(forItemID: id)) != nil else { return }
        openDrawing(itemID: id)
    }

    private func show(_ drawing: OpenDrawing) {
        // Deleted outside the app: back to the Gallery, without saving.
        drawing.keepsDeletedDrawingOpen = isCloudBacked
        drawing.onSaved = { [weak self] in self?.onLocalChange?() }
        drawing.onDeleted = { [weak self, weak drawing] in
            guard let self, open === drawing else { return }
            open = nil
            reload()
        }
        open = drawing
    }

    /// Saves the open Drawing and returns to the Gallery.
    func close() {
        open?.close()
        open = nil
        reload()
    }

    func dismissFailure() { failure = nil }

    private func attempt(_ message: LocalizedStringResource, _ work: () throws -> Void) {
        do {
            failure = nil
            try work()
        } catch let taken as DrawingStore.NameTaken {
            failure = String(localized: "“\(taken.name)” already exists. Choose another name.")
        } catch is DrawingStore.InvalidName {
            failure = String(localized: "That name cannot be used. Choose another name.")
        } catch {
            failure = String(localized: message)
        }
    }
}

/// A Drawing handed to the share sheet as its `.excalidraw` file, which is
/// looked up (and the open Drawing saved) only when something is shared.
struct ExportedDrawing: Transferable {
    let name: String
    let gallery: Gallery

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: .excalidraw) { drawing in
            guard let url = await drawing.gallery.exportFile(named: drawing.name) else {
                throw CocoaError(.fileNoSuchFile)
            }
            return SentTransferredFile(url)
        }
        .suggestedFileName { "\($0.name).excalidraw" }
    }
}

extension UTType {
    /// A `.excalidraw` file, as declared in the app's Info.plist.
    static let excalidraw = UTType(importedAs: "com.excalidraw.drawing", conformingTo: .json)
}
