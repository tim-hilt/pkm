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

    /// The pictures of the Drawings for their cards.
    @ObservationIgnored let thumbnails: Thumbnails

    @ObservationIgnored private let store: DrawingStore

    init(store: DrawingStore) {
        self.store = store
        thumbnails = Thumbnails(store: store)
        reload()
    }

    func reload() {
        let listed = (try? store.list()) ?? []
        if listed != drawings { drawings = listed }
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
            open = try OpenDrawing(name: name, store: store)
        }
    }

    /// Makes a new Drawing of each `.excalidraw` file (share sheet, Files,
    /// drag & drop), named after the file and made distinct if that name is
    /// taken; the files themselves are left alone. A file that cannot be read
    /// or is not a valid Drawing is skipped and the user is told. Returns the
    /// names of the new Drawings.
    @discardableResult
    func importDrawings(from urls: [URL]) -> [String] {
        failure = nil
        var imported: [String] = []
        var rejected: [String] = []
        for url in urls {
            if let name = try? importDrawing(from: url) {
                imported.append(name)
            } else {
                rejected.append("“\(url.lastPathComponent)”")
            }
        }
        if !rejected.isEmpty {
            failure = "Could not import \(rejected.joined(separator: ", ")). It is not a valid Excalidraw file."
        }
        reload()
        return imported
    }

    private func importDrawing(from url: URL) throws -> String {
        // Files from other apps (open-in, Files picker) need this; others ignore it.
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let contents = try Data(contentsOf: url)
        try Editor().load(bytes: contents)
        return try store.add(contents, named: url.deletingPathExtension().lastPathComponent)
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

    func openDrawing(named name: String) {
        attempt("Could not open “\(name)”.") {
            open = try OpenDrawing(name: name, store: store)
        }
    }

    /// Saves the open Drawing and returns to the Gallery.
    func close() {
        open?.saveNow()
        open = nil
        reload()
    }

    func dismissFailure() { failure = nil }

    private func attempt(_ message: String, _ work: () throws -> Void) {
        do {
            failure = nil
            try work()
        } catch let taken as DrawingStore.NameTaken {
            failure = "“\(taken.name)” already exists. Choose another name."
        } catch is DrawingStore.InvalidName {
            failure = "That name cannot be used. Choose another name."
        } catch {
            failure = message
        }
    }
}

extension UTType {
    /// A `.excalidraw` file, as declared in the app's Info.plist.
    static let excalidraw = UTType(importedAs: "com.excalidraw.drawing", conformingTo: .json)
}
