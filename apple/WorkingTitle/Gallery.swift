import Foundation
import Observation

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

    @ObservationIgnored private let store: DrawingStore

    init(store: DrawingStore) {
        self.store = store
        reload()
    }

    func reload() {
        drawings = (try? store.list()) ?? []
    }

    /// Makes a new empty Drawing and opens it.
    func createDrawing() {
        attempt("Could not create a Drawing.") {
            let name = try store.create(contents: Editor().save())
            open = try OpenDrawing(name: name, store: store)
        }
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
